use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    sync::Semaphore,
    task::JoinSet,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    LeaseExpired,
    Shutdown,
}

pub async fn serve(
    listener: TcpListener,
    guest: SocketAddr,
    allowed_sources: HashSet<IpAddr>,
    lease: Duration,
    max_connections: usize,
    shutdown: impl Future<Output = ()>,
) -> anyhow::Result<StopReason> {
    let deadline = tokio::time::sleep(lease);
    tokio::pin!(deadline);
    tokio::pin!(shutdown);
    let permits = Arc::new(Semaphore::new(max_connections));
    let mut connections = JoinSet::new();

    let reason = loop {
        tokio::select! {
            () = &mut deadline => break StopReason::LeaseExpired,
            () = &mut shutdown => break StopReason::Shutdown,
            accepted = listener.accept() => {
                let (mut client, peer) = accepted.context("failed to accept client")?;
                if !allowed_sources.contains(&peer.ip()) {
                    drop(client);
                    continue;
                }
                let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                    drop(client);
                    continue;
                };
                connections.spawn(async move {
                    let _permit = permit;
                    let mut upstream = TcpStream::connect(guest)
                        .await
                        .with_context(|| format!("failed to connect guest {guest}"))?;
                    copy_bidirectional(&mut client, &mut upstream)
                        .await
                        .with_context(|| format!("failed to proxy connection from {peer}"))?;
                    anyhow::Ok(())
                });
            }
            completed = connections.join_next(), if !connections.is_empty() => {
                match completed {
                    Some(Ok(Err(error))) => eprintln!("connection failed: {error:#}"),
                    Some(Err(error)) => eprintln!("connection task failed: {error}"),
                    Some(Ok(Ok(()))) | None => {}
                }
            }
        }
    };

    connections.abort_all();
    while connections.join_next().await.is_some() {}
    Ok(reason)
}

#[cfg(test)]
mod tests {
    use std::{future::pending, time::Duration};

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        time::timeout,
    };

    use super::*;

    async fn echo_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let (mut read, mut write) = stream.split();
                    let _ = tokio::io::copy(&mut read, &mut write).await;
                });
            }
        });
        (address, task)
    }

    async fn gateway(
        guest: SocketAddr,
        lease: Duration,
        max_connections: usize,
    ) -> (
        SocketAddr,
        tokio::task::JoinHandle<anyhow::Result<StopReason>>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(serve(
            listener,
            guest,
            HashSet::from([IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)]),
            lease,
            max_connections,
            pending(),
        ));
        (address, task)
    }

    #[tokio::test]
    async fn forwards_bytes_bidirectionally() {
        let (guest, echo_task) = echo_server().await;
        let (endpoint, gateway_task) = gateway(guest, Duration::from_secs(2), 3).await;
        let mut client = TcpStream::connect(endpoint).await.unwrap();

        client.write_all(b"adb").await.unwrap();
        let mut response = [0; 3];
        client.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"adb");

        gateway_task.abort();
        echo_task.abort();
    }

    #[tokio::test]
    async fn lease_expiry_closes_active_connections() {
        let (guest, echo_task) = echo_server().await;
        let (endpoint, gateway_task) = gateway(guest, Duration::from_millis(50), 3).await;
        let mut client = TcpStream::connect(endpoint).await.unwrap();

        assert_eq!(
            gateway_task.await.unwrap().unwrap(),
            StopReason::LeaseExpired
        );
        let mut byte = [0; 1];
        let count = timeout(Duration::from_secs(1), client.read(&mut byte))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(count, 0);
        echo_task.abort();
    }

    #[tokio::test]
    async fn connection_limit_rejects_excess_clients() {
        let (guest, echo_task) = echo_server().await;
        let (endpoint, gateway_task) = gateway(guest, Duration::from_secs(2), 1).await;
        let _first = TcpStream::connect(endpoint).await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut second = TcpStream::connect(endpoint).await.unwrap();
        let mut byte = [0; 1];
        let count = timeout(Duration::from_secs(1), second.read(&mut byte))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(count, 0);

        gateway_task.abort();
        echo_task.abort();
    }

    #[tokio::test]
    async fn rejects_source_outside_allowlist_before_contacting_guest() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = listener.local_addr().unwrap();
        let task = tokio::spawn(serve(
            listener,
            "127.0.0.1:9".parse().unwrap(),
            HashSet::from(["127.0.0.2".parse().unwrap()]),
            Duration::from_secs(2),
            3,
            pending(),
        ));
        let mut client = TcpStream::connect(endpoint).await.unwrap();
        let mut byte = [0; 1];
        let count = timeout(Duration::from_secs(1), client.read(&mut byte))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(count, 0);
        task.abort();
    }
}
