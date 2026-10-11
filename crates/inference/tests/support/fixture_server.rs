fn main() {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        time::{Duration, Instant},
    };
    let args = std::env::args().collect::<Vec<_>>();
    let context: u32 = args[args.iter().position(|a| a == "--ctx-size").unwrap() + 1]
        .parse()
        .unwrap();
    if context == 1024 {
        return;
    }
    let delay = if context == 512 {
        Duration::from_secs(20)
    } else {
        Duration::from_millis(500)
    };
    let port: u16 = args[args.iter().position(|a| a == "--port").unwrap() + 1]
        .parse()
        .unwrap();
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
    let start = Instant::now();
    for connection in listener.incoming() {
        let mut c = connection.unwrap();
        c.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut reader = BufReader::new(c.try_clone().unwrap());
        let mut request = String::new();
        reader.read_line(&mut request).unwrap();
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            match line.split_once(':') {
                Some((key, value)) if key.eq_ignore_ascii_case("content-length") => {
                    length = value.trim().parse::<usize>().unwrap()
                }
                _ => {}
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let (status, body) = if request.starts_with("GET /health ") {
            if start.elapsed() < delay {
                ("503 Service Unavailable", "{\"status\":\"loading model\"}")
            } else {
                ("200 OK", "{\"status\":\"ok\"}")
            }
        } else if request.starts_with("POST /v1/chat/completions ") {
            (
                "200 OK",
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"local works\"}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
            )
        } else {
            ("200 OK", "{\"data\":[]}")
        };
        let content_type = if request.starts_with("POST ") {
            "text/event-stream"
        } else {
            "application/json"
        };
        let _ = write!(
            c,
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    }
}
