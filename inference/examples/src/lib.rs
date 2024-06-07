// use std::vec;

// use inference_proc_macros::apply;
// use inference_std::address::Address;
// use inference_std::context::Context;

// fn sort(context: &mut Context, a: &mut Address, b: &mut Address) {
//     let mut mem_to_sort: Vec<&mut Address> = vec::Vec::new();
//     let mut find_start = false;

//     for address in context.mem.iter_mut() {
//         if !address.is_defined() {
//             continue;
//         }

//         if !find_start {
//             if address.value() == a.value() {
//                 mem_to_sort.push(a);
//                 find_start = true;
//             } else if address.value() == b.value() {
//                 mem_to_sort.push(b);
//                 find_start = true;
//             }
//         } else {
//             mem_to_sort.push(address);
//         }
//     }

//     mem_to_sort.sort_by(|a, b| a.data().cmp(&b.data()));
//     mem_to_sort
//         .iter()
//         .enumerate()
//         .for_each(|(i, address)| address.set_data(i));
// }

// type SortingFunctionType = fn(a: Address, b: Address);

// fn sort_spec() {
//     let context = Context::new("sort".to_string(), "program".to_string(), vec![], 0);

//     apply!(preserving_count(&context, sort));
//     apply!(procuring_sorted(&context, sort));
// }

// mod tests {
//     use super::*;

//     #[test]
//     fn test_sort_spec() {
//         sort_spec();
//     }
// }
