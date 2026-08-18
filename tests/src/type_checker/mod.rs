#[allow(clippy::module_inception)]
mod type_checker;

mod array_tests;
mod associated_functions;
mod coverage;
mod duplicate_parameter_name;
mod error_recovery;
mod extern_binding;
mod extern_index;
mod extern_name_collision;
mod features;
mod literal_typing;
mod multi_file;
mod multi_file_matrix;
mod named_call_arguments;
mod pow_rejection;
mod self_parameter_position;
mod spec_type_collision_diagnostics;
mod struct_tests;
mod type_info_tests;
