use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub fn address(listen: &str) -> Result<SocketAddr, std::net::AddrParseError> {
    let mut address: SocketAddr = listen.parse()?;
    if address.ip().is_unspecified() {
        address.set_ip(if address.is_ipv4() {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        } else {
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        });
    }
    Ok(address)
}
pub async fn check(listen: &str) -> Result<(), Box<dyn std::error::Error>> {
    let address = address(listen)?;
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut stream = tokio::net::TcpStream::connect(address).await?;
        stream
            .write_all(
                format!("GET /readyz HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await?;
        let mut status = [0; 12];
        stream.read_exact(&mut status).await?;
        if &status != b"HTTP/1.1 200" && &status != b"HTTP/1.0 200" {
            return Err(io::Error::other("server is not ready"));
        }
        Ok::<(), io::Error>(())
    })
    .await??;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn follows_port_and_address_family() {
        assert_eq!(
            address("0.0.0.0:9090").unwrap().to_string(),
            "127.0.0.1:9090"
        );
        assert_eq!(address("[::]:9091").unwrap().to_string(), "[::1]:9091");
        assert!(address("invalid").is_err());
    }
    #[tokio::test]
    async fn distinguishes_ready_and_unready() {
        for (status, ok) in [("200 OK", true), ("503 Unavailable", false)] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0; 256];
                let len = socket.read(&mut buf).await.unwrap();
                assert!(
                    std::str::from_utf8(&buf[..len])
                        .unwrap()
                        .starts_with("GET /readyz ")
                );
                socket
                    .write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n").as_bytes())
                    .await
                    .unwrap();
            });
            assert_eq!(check(&addr.to_string()).await.is_ok(), ok);
            server.await.unwrap();
        }
    }
}
