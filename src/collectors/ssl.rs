use rustls::{ClientConfig, ClientConnection, Stream};
use rustls::ServerName;
use std::io::Write;
use std::net::{TcpStream as StdTcpStream, ToSocketAddrs};
use std::sync::Arc as StdArc;
use anyhow::Context;
use std::time::SystemTime;

pub fn get_cert_expiry_seconds(domain: &str) -> anyhow::Result<f64> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.add_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.iter().map(|ta| {
        rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
            ta.subject,
            ta.spki,
            ta.name_constraints,
        )
    }));

    let config = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let server_name = ServerName::try_from(domain)
        .map_err(|_| anyhow::anyhow!("invalid DNS name"))?
        .to_owned();

    let mut conn = ClientConnection::new(StdArc::new(config), server_name)?;
    let port = 443;
    let addr = format!("{}:{}", domain, port);
    let mut socket_addrs = addr.to_socket_addrs()?;
    let socket_addr = socket_addrs.next().ok_or_else(|| anyhow::anyhow!("invalid DNS name"))?;
    let mut sock = StdTcpStream::connect_timeout(&socket_addr, std::time::Duration::from_secs(3))?;
    let mut stream = Stream::new(&mut conn, &mut sock);

    // Complete the handshake by writing something and flushing.
    // The handshake happens implicitly on the first read or write.
    stream.write_all(b"")?;
    stream.flush()?;

    let certs = stream.conn.peer_certificates()
        .context("No peer certificates found")?;

    let now = SystemTime::now();

    let mut min_expiry_seconds = f64::MAX;

    for cert_der in certs {
        let (_, cert) = x509_parser::parse_x509_certificate(cert_der.as_ref())
            .map_err(|e| anyhow::anyhow!("Failed to parse certificate: {}", e))?;
        let validity = cert.validity();
        let not_after = validity.not_after;
        let rfc2822_str = not_after.to_rfc2822().map_err(|e| anyhow::anyhow!(e))?;
        let not_after_dt = chrono::DateTime::parse_from_rfc2822(&rfc2822_str)?.with_timezone(&chrono::Utc);
        let not_after_st: SystemTime = not_after_dt.into();

        let duration_until_expiry = not_after_st.duration_since(now)?;
        let expiry_seconds = duration_until_expiry.as_secs_f64();

        if expiry_seconds < min_expiry_seconds {
            min_expiry_seconds = expiry_seconds;
        }
    }

    if min_expiry_seconds == f64::MAX {
        anyhow::bail!("Could not determine certificate expiry");
    }

    Ok(min_expiry_seconds.max(0.0))
}
