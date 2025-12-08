use actix_cors::Cors;
use actix_web::{post, web, App, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct CompileRequest {
    code: String,
}

#[derive(Deserialize, Serialize)]
struct Response {
    wat: String,
    wasm: Vec<u8>,
    wasm_str: String,
    v: String,
    errors: Vec<String>,
}

fn parse_inf_file(_: &str) -> Response {
    // let mut wasm = vec![];
    let v = String::new();
    let errors = vec![];

    // let wat = match compile_to_wat(input) {
    //     Ok(w) => w,
    //     Err(e) => {
    //         errors.push(e.to_string());
    //         return Response {
    //             wat: String::new(),
    //             wasm: vec![],
    //             wasm_str: String::new(),
    //             v: String::new(),
    //             errors,
    //         };
    //     }
    // };

    // if !wat.is_empty() {
    //     wat_to_wasm(&wat)
    //         .map(|w| wasm = w)
    //         .unwrap_or_else(|e| errors.push(e.to_string()));

    //     wasm_to_v("playground", &wasm)
    //         .map(|v_str| v = v_str)
    //         .unwrap_or_else(|e| errors.push(e.to_string()));

    //     let wat = wat_format(&wat);
    //     let wasm_str = wasm_format(&wasm);
    //     Response {
    //         wat,
    //         wasm,
    //         wasm_str,
    //         v,
    //         errors,
    //     }
    // } else {
    Response {
        wat: String::new(),
        wasm: vec![],
        wasm_str: String::new(),
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
