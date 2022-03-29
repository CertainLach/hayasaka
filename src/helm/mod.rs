mod template;

use crate::bail;
use fieldpath::FieldpathExt;
use gcmodule::Cc;
use jrsonnet_evaluator::gc::GcHashMap;
use jrsonnet_evaluator::throw;
use jrsonnet_evaluator::typed::Any;
use jrsonnet_evaluator::{
    error::Error::RuntimeError, error::Result, unwrap_type, Context, FuncVal, LazyBinding, LazyVal,
    ObjMember, ObjValue, Val,
};
use jrsonnet_macros::builtin;
use jrsonnet_parser::{ExprLocation, Visibility};
use jrsonnet_types::ty;
use serde_json::Value;
use std::fmt::Write;
use std::{convert::TryInto, thread};
use template::template_helm;

pub fn generate_key(val: &ObjValue, default_ns: &str) -> Result<String> {
    let mut out = String::new();

    let api_version = unwrap_type!(|| "apiVersion".into(), match val.get("apiVersion".into())? {
        Some(v) => v,
        None => bail!("missing apiVersion"),
    }, ty!(string) => Val::Str);
    out.push_str(&api_version);

    let kind = unwrap_type!(|| "kind".into(), match val.get("kind".into())? {
        Some(v) => v,
        None => bail!("missing kind"),
    }, ty!(string) => Val::Str);
    out.push(' ');
    out.push_str(&kind);

    let metadata = unwrap_type!(|| "metadata".into(), match val.get("metadata".into())? {
        Some(v) => v,
        None => bail!("missing object metadata"),
    }, ty!(object) => Val::Obj);
    let name = unwrap_type!(|| "name".into(), match metadata.get("name".into())? {
        Some(v) => v,
        None => bail!("missing name"),
    }, ty!(string) => Val::Str);
    out.push(' ');
    out.push_str(&name);
    if let Some(namespace) = metadata.get("namespace".into())? {
        let namespace = unwrap_type!(|| "namespace".into(), namespace, ty!(string) => Val::Str);
        if &namespace as &str != default_ns {
            out.push_str(" in ");
            out.push_str(&namespace);
        }
    }

    Ok(out)
}

pub fn obj_list_to_map(values: Vec<Val>, default_ns: &str) -> Result<Val> {
    let mut out = GcHashMap::with_capacity(values.len());

    for value in values {
        if matches!(&value, Val::Null) {
            continue;
        }
        let obj = unwrap_type!(|| "helm output item".into(), value, ty!(object) => Val::Obj);
        let name = generate_key(&obj, default_ns)?;

        let old = out.insert(
            name.clone().into(),
            ObjMember {
                add: false,
                visibility: Visibility::Normal,
                invoke: LazyBinding::Bound(LazyVal::new_resolved(Val::Obj(obj))),
                location: None,
            },
        );

        if old.is_some() {
            throw!(RuntimeError(
                format!("found duplicate objects with key {name}").into()
            ))
        }
    }

    Ok(Val::Obj(ObjValue::new(None, Cc::new(out), Cc::new(vec![]))))
}

pub fn helm_to_map(values: Vec<Val>, purifier: FuncVal, default_namespace: &str) -> Result<Val> {
    let mut out = GcHashMap::with_capacity(values.len());

    for value in values {
        if matches!(&value, Val::Null) {
            continue;
        }
        let obj = unwrap_type!(|| "helm output item".into(), value, ty!(object) => Val::Obj);
        let old_name = generate_key(&obj, default_namespace)?;
        let new_val = purifier.evaluate(
            Context::new(),
            None,
            &[Any(Val::Str(old_name.into())), Any(Val::Obj(obj))].as_slice(),
            false,
        )?;

        match new_val {
            Val::Null => continue,
            val => {
                let obj = unwrap_type!(|| "purifier output".into(), val, ty!(object) => Val::Obj);
                let new_name = generate_key(&obj, default_namespace)?;
                let old = out.insert(
                    new_name.into(),
                    ObjMember {
                        add: false,
                        visibility: Visibility::Normal,
                        invoke: LazyBinding::Bound(LazyVal::new_resolved(Val::Obj(obj))),
                        location: None,
                    },
                );
                assert!(old.is_none(), "duplicate object");
            }
        }
    }

    Ok(Val::Obj(ObjValue::new(None, Cc::new(out), Cc::new(vec![]))))
}

#[builtin]
pub fn helm_template(
    #[location] from: Option<&ExprLocation>,
    name: String,
    package: String,
    values: ObjValue,
    purifier: FuncVal,
    namespace: Option<String>,
    check_purity: Option<bool>,
) -> Result<Any> {
    let mut path = from.unwrap().0.to_path_buf();
    path.pop();

    let values = Value::try_from(&Val::Obj(values))?;
    let namespace = namespace.unwrap_or_else(|| todo!());
    let check_purity = check_purity.unwrap_or(true);

    // Spawn another thread, because helm is slow
    let helm_a = if check_purity {
        let name_a = name.to_owned().to_string();
        let package_a = package.to_string();
        let values_a = values.clone();

        let ns = namespace.to_owned();
        Some(thread::spawn(move || {
            template_helm(&ns, name_a.as_ref(), package_a.as_ref(), &values_a)
        }))
    } else {
        None
    };
    let helm_b_raw = template_helm(&namespace, name.as_ref(), package.as_ref(), &values)?;
    let helm_b_raw = helm_b_raw
        .iter()
        .map(Val::try_from)
        .collect::<Result<Vec<Val>>>()?;

    let helm_b = helm_to_map(helm_b_raw, purifier.clone(), &namespace)?;
    let helmval_b: Value = (&helm_b).try_into()?;

    let helmval_a = if let Some(helm_a) = helm_a {
        let helm_a_raw = helm_a.join().unwrap()?;
        let helm_a_raw = helm_a_raw
            .iter()
            .map(Val::try_from)
            .collect::<Result<Vec<Val>>>()?;

        let val = helm_to_map(helm_a_raw, purifier.clone(), &namespace)?;
        let value: Value = (&val).try_into()?;
        Some(value)
    } else {
        None
    };

    if let Some(helmval_a) = helmval_a {
        let json_patch = json_patch::diff(&helmval_a, &helmval_b);
        if !json_patch.0.is_empty() {
            let mut out = "impurity found between two helm runs:".to_owned();
            for p in json_patch.0 {
                match p {
                    json_patch::PatchOperation::Add(a) => {
                        write!(out, "\n+ {}", fieldpath::PathBuf::from_rfc6901(a.path))
                    }
                    json_patch::PatchOperation::Remove(r) => {
                        write!(out, "\n- {}", fieldpath::PathBuf::from_rfc6901(r.path))
                    }
                    json_patch::PatchOperation::Replace(r) => {
                        let path = fieldpath::PathBuf::from_rfc6901(r.path);
                        write!(
                            out,
                            "\nr {}\n  - {}\n  + {}",
                            path,
                            helmval_a.get_path(&path).unwrap(),
                            helmval_b.get_path(&path).unwrap()
                        )
                    }
                    _ => unreachable!(),
                }
                .unwrap()
            }
            crate::bail!("{}", out)
        }
    }

    Ok(Any(helm_b))
}
