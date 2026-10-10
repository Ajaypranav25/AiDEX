//! Test fixture: a fake `llama-server` speaking the subset of its HTTP API that
//! AiDEX uses (`/health`, `/v1/models`, `/v1/chat/completions` with SSE), so the
//! managed-runtime path can be tested without a real model or runtime.
//!
//! It requires `LLAMA_API_KEY` and checks `Authorization: Bearer` on every API
//! route except `/health`, like llama-server. Behaviour is driven by
//! `aidex.fake.*` metadata in the GGUF file passed with `--model`:
//! `startup_ms` (answer 503 on `/health` that long), `exit_code` (exit at
//! startup with that code), `token_delay_ms`, and `tokens`. `GET
//! /aidex-test/state` returns the recorded request bodies, argv, the number of
//! streams the client abandoned, and the environment variable *names* it saw;
//! it is unauthenticated because tests never learn the per-launch key.

use std::{
    env,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process::ExitCode,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

#[derive(Default)]
struct State {
    requests: Vec<Value>,
    abandoned: u64,
    completed: u64,
}

struct Server {
    key: String,
    args: Vec<String>,
    env_names: Vec<String>,
    started: Instant,
    startup: Duration,
    token_delay: Duration,
    tokens: u64,
    context: u64,
    model_name: String,
    state: Mutex<State>,
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(key) = env::var("LLAMA_API_KEY").ok().filter(|key| !key.is_empty()) else {
        eprintln!("fake-llama-server: LLAMA_API_KEY is not set");
        return ExitCode::from(3);
    };
    let Some(model) = arg_value(&args, "--model") else {
        eprintln!("fake-llama-server: --model is required");
        return ExitCode::from(2);
    };
    let metadata = match aidex_inference::gguf::read_metadata(std::path::Path::new(&model)) {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!("fake-llama-server: cannot read model: {error}");
            return ExitCode::from(4);
        }
    };
    let knob = |name: &str| {
        metadata
            .values
            .get(&format!("aidex.fake.{name}"))
            .and_then(|value| value.as_u64())
    };
    if let Some(code) = knob("exit_code") {
        eprintln!("fake-llama-server: simulated failure loading the model");
        return ExitCode::from(u8::try_from(code).unwrap_or(1));
    }
    let host = arg_value(&args, "--host").unwrap_or_else(|| "127.0.0.1".into());
    let port = arg_value(&args, "--port").unwrap_or_else(|| "8080".into());
    let listener = match TcpListener::bind(format!("{host}:{port}")) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("fake-llama-server: couldn't bind HTTP server socket: {error}");
            return ExitCode::from(1);
        }
    };
    eprintln!("fake-llama-server: listening on {host}:{port}");
    let context = arg_value(&args, "--ctx-size")
        .and_then(|value| value.parse().ok())
        .unwrap_or(4096);
    let server = Arc::new(Server {
        key,
        env_names: env::vars_os()
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect(),
        args,
        started: Instant::now(),
        startup: Duration::from_millis(knob("startup_ms").unwrap_or(0)),
        token_delay: Duration::from_millis(knob("token_delay_ms").unwrap_or(5)),
        tokens: knob("tokens").unwrap_or(4),
        context,
        model_name: metadata.name.unwrap_or_else(|| "fake".into()),
        state: Mutex::new(State::default()),
    });
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let server = Arc::clone(&server);
        thread::spawn(move || handle(&server, stream));
    }
    ExitCode::SUCCESS
}

struct Request {
    method: String,
    path: String,
    authorization: Option<String>,
    body: Vec<u8>,
}

fn read_request(stream: &TcpStream) -> Option<Request> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .ok()?;
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();
    let mut length = 0usize;
    let mut authorization = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 {
            return None;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().ok()?;
            } else if name.eq_ignore_ascii_case("authorization") {
                authorization = Some(value.trim().to_owned());
            }
        }
    }
    let mut body = vec![0u8; length.min(16 * 1024 * 1024)];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        path,
        authorization,
        body,
    })
}

fn respond(stream: &mut TcpStream, status: u16, body: &Value) {
    let text = body.to_string();
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    let _ = stream.flush();
}

fn handle(server: &Server, mut stream: TcpStream) {
    let Some(request) = read_request(&stream) else {
        return;
    };
    if request.method == "GET" && request.path == "/health" {
        if server.started.elapsed() < server.startup {
            respond(
                &mut stream,
                503,
                &json!({"error": {"code": 503, "message": "Loading model", "type": "unavailable_error"}}),
            );
        } else {
            respond(&mut stream, 200, &json!({"status": "ok"}));
        }
        return;
    }
    if request.method == "GET" && request.path == "/aidex-test/state" {
        let state = server.state.lock().map_or_else(
            |_| json!({}),
            |state| {
                json!({
                    "requests": state.requests,
                    "abandoned": state.abandoned,
                    "completed": state.completed,
                    "args": server.args,
                    "env_names": server.env_names,
                })
            },
        );
        respond(&mut stream, 200, &state);
        return;
    }
    let expected = format!("Bearer {}", server.key);
    if request.authorization.as_deref() != Some(expected.as_str()) {
        respond(
            &mut stream,
            401,
            &json!({"error": {"code": 401, "message": "Invalid API Key", "type": "authentication_error"}}),
        );
        return;
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/v1/models") => respond(
            &mut stream,
            200,
            &json!({"object": "list", "data": [{"id": server.model_name, "meta": {"n_ctx": server.context}}]}),
        ),
        ("POST", "/v1/chat/completions") => chat(server, &mut stream, &request.body),
        _ => respond(
            &mut stream,
            404,
            &json!({"error": {"code": 404, "message": "not found"}}),
        ),
    }
}

fn chat(server: &Server, stream: &mut TcpStream, body: &[u8]) {
    let Ok(request) = serde_json::from_slice::<Value>(body) else {
        respond(
            stream,
            400,
            &json!({"error": {"code": 400, "message": "invalid JSON"}}),
        );
        return;
    };
    if let Ok(mut state) = server.state.lock() {
        state.requests.push(request.clone());
    }
    let structured = request
        .pointer("/response_format/type")
        .and_then(Value::as_str)
        == Some("json_schema");
    let pieces: Vec<String> = if structured {
        vec!["{\"answer\":".into(), " \"fake\"}".into()]
    } else {
        (0..server.tokens)
            .map(|index| format!("tok{index} "))
            .collect()
    };
    let prompt_tokens = request["messages"]
        .as_array()
        .map_or(0, |messages| messages.len() as u64 * 4);
    let completion_tokens = pieces.len() as u64;
    if !request["stream"].as_bool().unwrap_or(false) {
        thread::sleep(server.token_delay * u32::try_from(completion_tokens).unwrap_or(1));
        respond(
            stream,
            200,
            &json!({
                "object": "chat.completion",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": pieces.concat()}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens},
                "timings": {
                    "prompt_n": prompt_tokens, "prompt_ms": 2.0, "prompt_per_second": prompt_tokens as f64 * 500.0,
                    "predicted_n": completion_tokens, "predicted_ms": 10.0, "predicted_per_second": completion_tokens as f64 * 100.0
                }
            }),
        );
        if let Ok(mut state) = server.state.lock() {
            state.completed += 1;
        }
        return;
    }
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
    let mut ok = stream.write_all(head.as_bytes()).is_ok();
    let mut send = |payload: &str| -> bool {
        stream
            .write_all(format!("data: {payload}\n\n").as_bytes())
            .and_then(|()| stream.flush())
            .is_ok()
    };
    ok =
        ok && send(&json!({"choices": [{"index": 0, "delta": {"role": "assistant"}}]}).to_string());
    for piece in &pieces {
        if !ok {
            break;
        }
        thread::sleep(server.token_delay);
        ok = send(&json!({"choices": [{"index": 0, "delta": {"content": piece}}]}).to_string());
    }
    if ok {
        ok = send(&json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]}).to_string())
            && send(&json!({"choices": [], "usage": {"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens}}).to_string())
            && send("[DONE]");
    }
    if let Ok(mut state) = server.state.lock() {
        if ok {
            state.completed += 1;
        } else {
            state.abandoned += 1;
        }
    }
}
