// use serde_json::json;
// use serde_json::Value;

// use super::find::Object;
// use crate::apply::{find::ObjectLocation, ObjectKind};
// use std::cmp::Ordering;
// use std::collections::HashSet;
// use topological_sort::TopologicalSort;

// fn is_crd(obj: &Value) -> bool {
//     match &obj["kind"]["api_version"] {
//         Value::String(s) if s.starts_with("apiextensions.k8s.io/") => {}
//         _ => return false,
//     }
//     obj["kind"]["name"] == "CustomResourceDefinition"
// }
// fn is_namespace(obj: &Value) -> bool {
//     match &obj["kind"]["api_version"] {
//         Value::String(s) if !s.contains("/") => {}
//         _ => return false,
//     }
//     obj["kind"]["name"] == "Namespace"
// }

// pub fn retain_split(i: Vec<Value>, pred: impl Fn(&Value) -> bool) -> (Vec<Value>, Vec<Value>) {
//     let mut a = Vec::new();
//     let mut b = Vec::new();

//     for v in i.into_iter() {
//         if pred(&v) {
//             a.push(v)
//         } else {
//             b.push(v);
//         }
//     }

//     (a, b)
// }

// // TODO: real topsort
// pub fn topsort_values(target: Vec<Value>) -> Vec<Vec<Value>> {
//     // Namespaces can't depend on crds, and vice versa, so it is safe to split them into separate group
//     let (crds_nses, rest) = retain_split(target, |o| is_namespace(&o) || is_crd(&o));

//     vec![crds_nses, rest]
// }

// #[test]
// fn test() {
//     dbg!(topsort_values(vec![
//         json! {{
//             "kind": json! {{
//                 "version": "apiextensions.k8s.io/v1",
//                 "name": "CustomResourceDefinition",
//             }}
//         }},
//         json! {{
//             "kind": json! {{}},
//         }},
//         json! {{
//             "kind": json! {{

//             }},
//         }},
//     ]))
// }
