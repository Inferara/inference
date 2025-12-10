//TODO: don't forget to remove
#![allow(dead_code)]
use inference_ast::nodes::{Expression, FunctionDefinition, Literal, Statement, Type};
use inkwell::{builder::Builder, context::Context, module::Module, values::FunctionValue};
use std::{process::Command, rc::Rc};
use tempfile::tempdir;

const UZUMAKI_I32_INTRINSIC: &str = "llvm.wasm.uzumaki.i32";
const UZUMAKI_I64_INTRINSIC: &str = "llvm.wasm.uzumaki.i64";
const FORALL_START_INTRINSIC: &str = "llvm.wasm.forall.start";
const FORALL_END_INTRINSIC: &str = "llvm.wasm.forall.end";
const EXISTS_START_INTRINSIC: &str = "llvm.wasm.exists.start";
const EXISTS_END_INTRINSIC: &str = "llvm.wasm.exists.end";
const ASSUME_START_INTRINSIC: &str = "llvm.wasm.assume.start";
const ASSUME_END_INTRINSIC: &str = "llvm.wasm.assume.end";
const UNIQUE_START_INTRINSIC: &str = "llvm.wasm.unique.start";
const UNIQUE_END_INTRINSIC: &str = "llvm.wasm.unique.end";

pub(crate) struct Compiler<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
}

impl<'ctx> Compiler<'ctx> {
    pub(crate) fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();

        Self {
            context,
            module,
            builder,
        }
    }

    pub(crate) fn visit_function_definition(&self, function_definition: &Rc<FunctionDefinition>) {
        let fn_name = function_definition.name();
        let fn_type = match &function_definition.returns {
            Some(ret_type) => match ret_type {
                Type::Array(_array_type) => todo!(),
                Type::Simple(simple_type) => match simple_type.name.to_lowercase().as_str() {
                    "i32" => self.context.i32_type().fn_type(&[], false),
                    "i64" => self.context.i64_type().fn_type(&[], false),
                    "u32" => todo!(),
                    "u64" => todo!(),
                    _ => panic!("Unsupported return type: {}", simple_type.name),
                },
                Type::Generic(_generic_type) => todo!(),
                Type::Function(_function_type) => todo!(),
                Type::QualifiedName(_qualified_name) => todo!(),
                Type::Qualified(_type_qualified_name) => todo!(),
                Type::Custom(_identifier) => todo!(),
            },
            None => self.context.void_type().fn_type(&[], false),
        };
        let function = self.module.add_function(fn_name.as_str(), fn_type, None);

        let export_name_attr = self
            .context
            .create_string_attribute("wasm-export-name", fn_name.as_str());
        function.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            export_name_attr,
        );

        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);
        self.visit_statements(function_definition.body.statements());
    }

    fn visit_statements(&self, statements: Vec<Statement>) {
        for stmt in statements {
            match stmt {
                Statement::Block(_block_type) => todo!(),
                Statement::Expression(_expression) => todo!(),
                Statement::Assign(_assign_statement) => todo!(),
                Statement::Return(return_statement) => {
                    let ret = match &*return_statement.expression.borrow() {
                        Expression::ArrayIndexAccess(_array_index_access_expression) => todo!(),
                        Expression::Binary(_binary_expression) => todo!(),
                        Expression::MemberAccess(_member_access_expression) => todo!(),
                        Expression::TypeMemberAccess(_type_member_access_expression) => todo!(),
                        Expression::FunctionCall(_function_call_expression) => todo!(),
                        Expression::Struct(_struct_expression) => todo!(),
                        Expression::PrefixUnary(_prefix_unary_expression) => todo!(),
                        Expression::Parenthesized(_parenthesized_expression) => todo!(),
                        Expression::Literal(literal) => match literal {
                            Literal::Array(_array_literal) => todo!(),
                            Literal::Bool(_bool_literal) => todo!(),
                            Literal::String(_string_literal) => todo!(),
                            Literal::Number(number_literal) => self
                                .context
                                .i32_type()
                                .const_int(number_literal.value.parse::<u64>().unwrap_or(0), false),
                            Literal::Unit(_unit_literal) => todo!(),
                        },
                        Expression::Identifier(_identifier) => todo!(),
                        Expression::Type(_) => todo!(),
                        Expression::Uzumaki(_uzumaki_expression) => todo!(),
                    };
                    self.builder.build_return(Some(&ret)).unwrap();
                }
                Statement::Loop(_loop_statement) => todo!(),
                Statement::Break(_break_statement) => todo!(),
                Statement::If(_if_statement) => todo!(),
                Statement::VariableDefinition(_variable_definition_statement) => todo!(),
                Statement::TypeDefinition(_type_definition_statement) => todo!(),
                Statement::Assert(_assert_statement) => todo!(),
                Statement::ConstantDefinition(_constant_definition) => todo!(),
            }
        }
    }

    fn uzumaki_i32_intrinsic(&self) -> FunctionValue<'ctx> {
        let i32_type = self.context.i32_type();
        let fn_type = i32_type.fn_type(&[], false);
        self.module
            .get_function(UZUMAKI_I32_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(UZUMAKI_I32_INTRINSIC, fn_type, None)
            })
    }

    fn uzumaki_i64_intrinsic(&self) -> FunctionValue<'ctx> {
        let i64_type = self.context.i64_type();
        let fn_type = i64_type.fn_type(&[], false);
        self.module
            .get_function(UZUMAKI_I64_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(UZUMAKI_I64_INTRINSIC, fn_type, None)
            })
    }

    fn forall_start_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(FORALL_START_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(FORALL_START_INTRINSIC, fn_type, None)
            })
    }

    fn forall_end_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(FORALL_END_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(FORALL_END_INTRINSIC, fn_type, None)
            })
    }

    fn exists_start_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(EXISTS_START_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(EXISTS_START_INTRINSIC, fn_type, None)
            })
    }

    fn exists_end_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(EXISTS_END_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(EXISTS_END_INTRINSIC, fn_type, None)
            })
    }

    fn assume_start_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(ASSUME_START_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(ASSUME_START_INTRINSIC, fn_type, None)
            })
    }

    fn assume_end_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(ASSUME_END_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(ASSUME_END_INTRINSIC, fn_type, None)
            })
    }

    fn unique_start_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(UNIQUE_START_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(UNIQUE_START_INTRINSIC, fn_type, None)
            })
    }

    fn unique_end_intrinsic(&self) -> FunctionValue<'ctx> {
        let void_type = self.context.void_type();
        let fn_type = void_type.fn_type(&[], false);
        self.module
            .get_function(UNIQUE_END_INTRINSIC)
            .unwrap_or_else(|| {
                self.module
                    .add_function(UNIQUE_END_INTRINSIC, fn_type, None)
            })
    }

    // Compilation methods
    pub(crate) fn compile_to_wasm(
        &self,
        output_fname: &str,
        optimization_level: u32,
    ) -> anyhow::Result<Vec<u8>> {
        let llc_path = Compiler::get_inf_llc_path()?;
        let temp_dir = tempdir()?;
        let output_path = temp_dir.path().join(output_fname);
        let ir_path = output_path.with_extension("ll");

        let ir_str = self.module.print_to_string().to_string();
        std::fs::write(&ir_path, ir_str)?;
        let opt_flag = format!("-O{}", optimization_level.min(3));
        let output = Command::new(&llc_path)
            .arg("-march=wasm32")
            .arg("-mcpu=mvp")
            .arg("-filetype=obj")
            .arg(&ir_path)
            .arg(&opt_flag)
            .arg("-o")
            .arg(&output_path)
            .output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "inf-llc failed with status: {}\nstderr: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let wasm_bytes = std::fs::read(&output_path)?;
        std::fs::remove_file(output_path)?;
        Ok(wasm_bytes)
    }

    fn get_inf_llc_path() -> anyhow::Result<std::path::PathBuf> {
        let llc_name = if cfg!(windows) { "llc.exe" } else { "llc" };

        let exe_path = std::env::current_exe()
            .map_err(|e| anyhow::anyhow!("Failed to get current executable path: {e}"))?;

        let exe_dir = exe_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Failed to get executable directory"))?;

        // Try multiple possible locations:
        // 1. For regular binaries: <exe_dir>/bin/llc
        // 2. For test binaries in deps/: <exe_dir>/../bin/llc
        let candidates = vec![
            exe_dir.join("bin").join(llc_name), // target/debug/bin/llc or target/release/bin/llc
            exe_dir.parent().map_or_else(
                || exe_dir.join("bin").join(llc_name),
                |p| p.join("bin").join(llc_name), // target/debug/bin/llc when exe is in target/debug/deps/
            ),
        ];

        for llc_path in &candidates {
            if llc_path.exists() {
                return Ok(llc_path.clone());
            }
        }

        Err(anyhow::anyhow!(
            "🚫 Inference llc binary not found\n\
            \n\
            This package requires LLVM with custom intrinsics support.\n\n\
            Executable: {}\n\
            Searched locations:\n  - {}\n  - {}",
            exe_path.display(),
            candidates[0].display(),
            candidates[1].display()
        ))
    }
}
