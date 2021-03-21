mod template;

use crate::bail;
use fieldpath::FieldpathExt;
use jrsonnet_evaluator::{
    error::Result, native::NativeCallback, unwrap_type, Context, FuncVal, LazyBinding, LazyVal,
    ObjMember, ObjValue, Val,
};
use jrsonnet_parser::{Param, ParamsDesc, Visibility};
use jrsonnet_types::ty;
use rustc_hash::FxHashMap;
use serde_json::Value;
use std::fmt::Write;
use std::{convert::TryInto, hash::BuildHasherDefault, path::PathBuf, rc::Rc, thread};
use template::template_helm;

pub fn create_helm_template(namespace: Rc<str>) -> NativeCallback {
    NativeCallback::new(
        ParamsDesc(Rc::new(vec![
            Param("a".into(), None),
            Param("b".into(), None),
            Param("c".into(), None),
            Param("d".into(), None),
        ])),
        move |path, args| helm_template(&namespace, path, args, true),
    )
}

pub fn generate_key(val: &ObjValue) -> Result<String> {
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
        out.push_str(" in ");
        out.push_str(&namespace);
    }

    Ok(out)
}

fn helm_to_map(values: Vec<Value>, purifier: Rc<FuncVal>) -> Result<Val> {
    let mut out = FxHashMap::with_capacity_and_hasher(values.len(), BuildHasherDefault::default());

    for value in values {
        if matches!(&value, Value::Null) {
            continue;
        }
        let val: Val = (&value).into();
        let obj = unwrap_type!(|| "helm output item".into(), val, ty!(object) => Val::Obj);
        let old_name = generate_key(&obj)?;
        let new_val = purifier
            .evaluate_values(Context::new(), &[Val::Str(old_name.into()), Val::Obj(obj)])?;

        match new_val {
            Val::Null => continue,
            val => {
                let obj = unwrap_type!(|| "purifier output".into(), val, ty!(object) => Val::Obj);
                let new_name = generate_key(&obj)?;
                out.insert(
                    new_name.into(),
                    ObjMember {
                        add: false,
                        visibility: Visibility::Normal,
                        invoke: LazyBinding::Bound(LazyVal::new_resolved(Val::Obj(obj))),
                        location: None,
                    },
                );
            }
        }
    }

    Ok(Val::Obj(ObjValue::new(None, Rc::new(out))))
}

fn helm_template(
    namespace: &str,
    path: Option<Rc<PathBuf>>,
    args: &[Val],
    check_purity: bool,
) -> jrsonnet_evaluator::error::Result<Val> {
    let mut path = PathBuf::clone(&path.expect("caller path should be present"));
    path.pop();
    let name = unwrap_type!(|| "name".to_owned(), args[0].clone(), ty!(string) => Val::Str);
    let package = unwrap_type!(|| "package".to_owned(), args[1].clone(), ty!(string) => Val::Str);
    let values: serde_json::Value =
        (&Val::Obj(unwrap_type!(|| "values".to_owned(), args[2].clone(), ty!(object) => Val::Obj)))
            .try_into()?;
    let purifier =
        unwrap_type!(|| "purifier".to_owned(), args[3].clone(), ty!(function) => Val::Func);

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
    let helm_b = helm_to_map(
        template_helm(namespace, name.as_ref(), package.as_ref(), &values)?,
        purifier.clone(),
    )?;
    let helmval_b: Value = (&helm_b).try_into()?;

    let helmval_a = if let Some(helm_a) = helm_a {
        let val = helm_to_map(helm_a.join().unwrap()?, purifier.clone())?;
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
                        write!(out, "\nr {}\n  - {}\n  + {}", path, helmval_a.get_path(&path).unwrap(), helmval_b.get_path(&path).unwrap())
                    }
                    _ => unreachable!(),
                }
                .unwrap()
            }
            crate::bail!("{}", out)
        }
    }

    Ok(helm_b)
}
