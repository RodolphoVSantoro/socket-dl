use std::{
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpSocket,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    dotenvy::dotenv().ok();
    let env_map = dotenvy::vars().collect::<std::collections::HashMap<_, _>>();

    let default_port = "9999".to_string();
    let port = env_map.get("PORT").unwrap_or(&default_port);

    let addr: std::net::SocketAddr = format!("0.0.0.0:{port}").parse().unwrap();
    let socket = TcpSocket::new_v4().expect("Failed to create socket");
    socket
        .set_reuseaddr(true)
        .expect("Failed to set reuse address");
    socket.bind(addr).expect("Failed to bind to address");

    let listener = socket.listen(2).expect("Failed to listen on address");

    while let Ok((mut stream, _)) = listener.accept().await {
        let request_buffer = &mut [0; 512];
        let read_result = stream.read(request_buffer).await;
        let request_size = match read_result {
            Ok(request_size) => request_size,
            Err(error) => {
                respond_with_error(
                    &mut stream,
                    &format!("Err: Failed to read from tcp stream: {}", error),
                )
                .await;
                continue;
            }
        };
        if request_size > 500 {
            respond_with_error(
                &mut stream,
                &format!("Err: Request size is too large: {}", request_size),
            )
            .await;
            continue;
        }

        let file = String::from_utf8_lossy(&request_buffer[..request_size]).to_string();

        if file.contains("/") || file.contains("..") {
            respond_with_error(&mut stream, &format!("Invalid file {}", file)).await;
            continue;
        }

        println!("File requested: {}", file);
        let folder_path = env_map.get("FOLDER_PATH").unwrap();
        println!("Folder path: {}", folder_path);
        let path = PathBuf::from(folder_path);
        let file = read_file(path, &file);
        send_file(file, &mut stream).await;
    }
}

async fn send_file(mut file: BufReader<File>, stream: &mut tokio::net::TcpStream) {
    let file_size = if file.get_ref().metadata().is_ok() {
        file.get_ref().metadata().unwrap().len()
    } else {
        respond_with_error(stream, "Err: Failed to get file metadata").await;
        return;
    };
    let chunk_amount = (file_size as f64 / 1024.0).ceil() as u64;
    println!(
        "File size: {} bytes, Chunk amount: {}",
        file_size, chunk_amount
    );

    let err = stream.write_all(&chunk_amount.to_be_bytes()).await;
    if let Err(error) = err {
        let _ = stream.write_all(&[]).await;
        println!("Err: Failed to write to tcp stream: {}", error);
        return;
    }

    loop {
        let mut chunk = [0; 1024];
        let bytes_read = file.read(&mut chunk).expect("Failed to read from file");
        if bytes_read == 0 {
            let _ = stream.write_all(&[]).await;
            break;
        }
        let res = stream.write_all(&chunk[..bytes_read]).await;
        if let Err(error) = res {
            let _ = stream.write_all(&[]).await;
            println!("Err: Failed to write to tcp stream: {}", error);
            break;
        }
    }
}

fn read_file(path: PathBuf, file_name: &str) -> BufReader<File> {
    let file_path = path.join(file_name);
    let in_file = File::open(file_path).unwrap();
    let buffer = BufReader::new(in_file);
    return buffer;
}

async fn respond_with_error(stream: &mut tokio::net::TcpStream, error: &str) {
    let _ = stream.write_all(error.as_bytes()).await;
    let _ = stream.write_all(&[]).await;
    println!("Err: {}", error);
}
