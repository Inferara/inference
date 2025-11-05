use std::rc::Rc;

use inference_ast::nodes::{Expression, FunctionDefinition, Literal, Statement};
use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function, FunctionSection, Module, TypeSection, ValType,
};

#[derive(Default)]
pub struct WasmModuleBuilder {
    module: Module,
    types: TypeSection,
    functions: FunctionSection,
    exports: ExportSection,
    codes: CodeSection,

    next_type_index: u32,
}

impl WasmModuleBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            module: Module::new(),
            types: TypeSection::new(),
            functions: FunctionSection::new(),
            exports: ExportSection::new(),
            codes: CodeSection::new(),
            next_type_index: 0,
        }
    }

    #[allow(unused_variables)]
    pub fn push_function(&mut self, function_definition: &Rc<FunctionDefinition>) -> u32 {
        let function_type_index = self.next_type_index;
        self.next_type_index += 1;
        self.functions.function(function_type_index);
        self.exports.export(
            function_definition.name().as_str(),
            ExportKind::Func,
            function_type_index,
        );
        let params: Vec<(u32, ValType)> = vec![];
        // if let Some(args) = &function_definition.arguments {
        //     let mut arg_index = 0;
        //     for param in args {
        //         match param {
        //             ArgumentType::SelfReference(self_reference) => todo!(),
        //             ArgumentType::IgnoreArgument(ignore_argument) => todo!(),
        //             ArgumentType::Argument(argument) => {
        //                 let t = match &argument.ty {
        //                     Type::Array(type_array) => todo!(),
        //                     Type::Simple(simple_type) => {
        //                         simple_type.name
        //                     }
        //                     Type::Generic(generic_type) => todo!(),
        //                     Type::Function(function_type) => todo!(),
        //                     Type::QualifiedName(qualified_name) => todo!(),
        //                     Type::Qualified(type_qualified_name) => todo!(),
        //                     Type::Custom(identifier) => todo!(),
        //                 };
        //             }
        //             ArgumentType::Type(ty) => match ty {
        //                 Type::Array(type_array) => todo!(),
        //                 Type::Simple(simple_type) => todo!(),
        //                 Type::Generic(generic_type) => todo!(),
        //                 Type::Function(function_type) => todo!(),
        //                 Type::QualifiedName(qualified_name) => todo!(),
        //                 Type::Qualified(type_qualified_name) => todo!(),
        //                 Type::Custom(identifier) => todo!(),
        //             },
        //         }
        //     }
        // }
        let mut function = Function::new(params);
        for stmt in function_definition.body.statements() {
            match stmt {
                Statement::Block(block_type) => todo!(),
                Statement::Expression(expression) => todo!(),
                Statement::Assign(assign_statement) => todo!(),
                Statement::Return(return_statement) => {
                    match &*return_statement.expression.borrow() {
                        Expression::ArrayIndexAccess(array_index_access_expression) => todo!(),
                        Expression::Binary(binary_expression) => todo!(),
                        Expression::MemberAccess(member_access_expression) => todo!(),
                        Expression::TypeMemberAccess(type_member_access_expression) => todo!(),
                        Expression::FunctionCall(function_call_expression) => todo!(),
                        Expression::Struct(struct_expression) => todo!(),
                        Expression::PrefixUnary(prefix_unary_expression) => todo!(),
                        Expression::Parenthesized(parenthesized_expression) => todo!(),
                        Expression::Literal(literal) => match literal {
                            Literal::Array(array_literal) => todo!(),
                            Literal::Bool(bool_literal) => todo!(),
                            Literal::String(string_literal) => todo!(),
                            Literal::Number(number_literal) => {
                                function.instruction(&wasm_encoder::Instruction::I32Const(
                                    number_literal.value.parse::<i32>().unwrap_or(0),
                                ));
                            }
                            Literal::Unit(unit_literal) => todo!(),
                        },
                        Expression::Identifier(identifier) => todo!(),
                        Expression::Type(_) => todo!(),
                        Expression::Uzumaki(uzumaki_expression) => todo!(),
                    }
                    function.instruction(&wasm_encoder::Instruction::Return);
                }
                Statement::Loop(loop_statement) => todo!(),
                Statement::Break(break_statement) => todo!(),
                Statement::If(if_statement) => todo!(),
                Statement::VariableDefinition(variable_definition_statement) => todo!(),
                Statement::TypeDefinition(type_definition_statement) => todo!(),
                Statement::Assert(assert_statement) => todo!(),
                Statement::ConstantDefinition(constant_definition) => todo!(),
            }
        }
        self.codes.function(&function);
        function_type_index
    }

    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        // Sections must be appended in canonical order:
        self.module.section(&self.types);
        self.module.section(&self.functions);
        self.module.section(&self.exports);
        self.module.section(&self.codes);
        self.module.finish()
    }
}

#[cfg(test)]
mod tests {
    use wasm_encoder::{Function, ValType};
    use wasmtime::{Caller, Engine, Store};

    use super::*;

    #[test]
    fn test_wasm_module_builder() -> anyhow::Result<()> {
        let mut module = Module::new();
        let params = vec![];
        let results = vec![ValType::I32];

        let mut types = TypeSection::new();
        types.ty().function(params, results);
        module.section(&types);

        let mut functions = FunctionSection::new();
        let type_index = 0;
        functions.function(type_index);
        module.section(&functions);

        let mut exports = ExportSection::new();
        exports.export("helloWorld", ExportKind::Func, 0);
        module.section(&exports);

        let mut codes = CodeSection::new();
        let mut f = Function::new(vec![]);
        f.instructions().i32_const(42).end();
        codes.function(&f);
        module.section(&codes);

        let wasm_bytes = module.finish();

        let engine = Engine::default();
        let module = wasmtime::Module::new(&engine, &wasm_bytes).unwrap();
        let mut linker = wasmtime::Linker::new(&engine);
        linker.func_wrap(
            "host",
            "host_func",
            |caller: Caller<'_, i32>, param: i32| {
                println!("Got {param} from WebAssembly");
                println!("my host state is: {}", caller.data());
            },
        )?;
        let mut store = Store::new(&engine, 4);
        let instance = linker.instantiate(&mut store, &module)?;
        let hello = instance.get_typed_func::<(), i32>(&mut store, "helloWorld")?;
        let result = hello.call(&mut store, ())?;
        assert_eq!(result, 42);
        Ok(())
    }

    #[test]
    fn test_module_bitwise_reproducable() {
        let mut previous: Option<Vec<u8>> = None;
        for _ in 0..10 {
            let mut module = Module::new();
            let params = vec![];
            let results = vec![ValType::I32];

            let mut types = TypeSection::new();
            types.ty().function(params, results);
            module.section(&types);

            let mut functions = FunctionSection::new();
            let type_index = 0;
            functions.function(type_index);
            module.section(&functions);

            let mut exports = ExportSection::new();
            exports.export("helloWorld", ExportKind::Func, 0);
            module.section(&exports);

            let mut codes = CodeSection::new();
            let mut f = Function::new(vec![]);
            f.instructions().i32_const(42).end();
            codes.function(&f);
            module.section(&codes);

            let wasm_bytes = module.finish();
            if let Some(prev) = previous {
                assert_eq!(prev.len(), wasm_bytes.len());
                for (b1, b2) in prev.iter().zip(wasm_bytes.iter()) {
                    assert_eq!(b1, b2);
                }
            }
            previous = Some(wasm_bytes);
        }
    }
}
