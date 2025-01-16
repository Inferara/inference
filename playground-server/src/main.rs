use actix_cors::Cors;
use actix_web::{post, web, App, HttpResponse, HttpServer, Responder};
use inference::{compile_to_wat, wasm_to_coq, wat_to_wasm};
use serde::{Deserialize, Serialize};

use wat_fmt::format;

#[derive(Deserialize)]
struct CompileRequest {
    code: String,
}

#[derive(Deserialize, Serialize)]
struct Response {
    wat: String,
    wasm: Vec<u8>,
    v: String,
    errors: Vec<String>,
}

fn parse_inf_file(input: &str) -> Response {
    let mut wasm = vec![];
    let mut v = String::new();
    let mut errors = vec![];

    let wat = match compile_to_wat(input) {
        Ok(w) => w,
        Err(e) => {
            errors.push(e.to_string());
            return Response {
                wat: String::new(),
                wasm: vec![],
                v: String::new(),
                errors,
            };
        }
    };

    if !wat.is_empty() {
        wat_to_wasm(&wat)
            .map(|w| wasm = w)
            .unwrap_or_else(|e| errors.push(e.to_string()));
        wasm_to_coq(&wasm)
            .map(|v_str| v = v_str)
            .unwrap_or_else(|e| errors.push(e.to_string()));

        let wat = format(&wat);
        Response {
            wat,
            wasm,
            v,
            errors,
        }
    } else {
        Response {
            wat: String::new(),
            wasm: vec![],
            v,
            errors,
        }
    }
}

#[post("/compile")]
async fn compile_code(payload: web::Json<CompileRequest>) -> impl Responder {
    let code = &payload.code;
    let compiled_result = parse_inf_file(code);
    HttpResponse::Ok().json(compiled_result)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .wrap(
                Cors::default()
                    .allowed_origin("http://localhost:3000")
                    .allowed_methods(vec!["POST", "GET"])
                    .allowed_headers(vec!["Content-Type"])
                    .supports_credentials(),
            )
            .service(compile_code)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
