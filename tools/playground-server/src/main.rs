use actix_cors::Cors;
use actix_web::{App, HttpResponse, HttpServer, Responder, post, web};
use serde::{Deserialize, Serialize};

use inference::{codegen, parse, wasm_to_v};

#[derive(Deserialize)]
struct CompileRequest {
    code: String,
}

#[derive(Deserialize, Serialize)]
struct Response {
    ll: String,
    wasm: Vec<u8>,
    wasm_str: String,
    v: String,
    errors: Vec<String>,
}

fn parse_inf_file(inf_code: &str) -> Response {
    let errors = vec![];

    let parse_result = match parse(inf_code) {
        Ok(result) => result,
        Err(e) => {
            return Response {
                ll: String::new(),
                wasm: vec![],
                wasm_str: String::new(),
                v: String::new(),
                errors: vec![e.to_string()],
            };
        }
    };

    let wasm_bytes = match codegen(&parse_result) {
        Ok(bytes) => bytes,
        Err(e) => {
            return Response {
                ll: String::new(),
                wasm: vec![],
                wasm_str: String::new(),
                v: String::new(),
                errors: vec![e.to_string()],
            };
        }
    };

    let v = match wasm_to_v("playground", &wasm_bytes) {
        Ok(v_str) => v_str,
        Err(e) => {
            return Response {
                ll: String::new(),
                wasm: vec![],
                wasm_str: String::new(),
                v: String::new(),
                errors: vec![e.to_string()],
            };
        }
    };

    Response {
        ll: String::new(),
        wasm: wasm_bytes.clone(),
        wasm_str: wasm_bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>(),
        v,
        errors,
    }
    // }
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
                    .allowed_origin("http://localhost:3001")
                    .allowed_methods(vec!["POST", "GET"])
                    .allowed_headers(vec!["Content-Type"])
                    .supports_credentials(),
            )
            .service(compile_code)
    })
    .bind(("127.0.0.1", 8181))?
    .run()
    .await
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_inf_file() {
        let input = r#"
        fn main() {
            let x: i32 = 10;
            let y: i32 = 20;
            let z: i32 = x + y;
        }
        "#;
        parse_inf_file(input);
        // assert_eq!(result.errors.len(), 0);
        // assert_eq!(result.wat.len(), 0);
        // assert_eq!(result.v.len(), 0);
        // assert_eq!(result.wasm.len(), 0);
    }
}
