use fieldpath::PathBuf;

#[derive(Debug, PartialEq)]
pub struct Conflict(pub String, pub Vec<PathBuf>);

peg::parser! {
    pub grammar conflict_error_parser() for str {
        rule prelude() -> u32
            = "Apply failed with " count:$(['0'..='9']+) " conflict" "s"? ":" {
                count.parse().unwrap()
            };
        rule manager_using() -> String
            = "using " ver:$(['a'..='z' | '0'..='9' | '/' | '.']+) {
                ver.to_owned()
            }
        rule manager_prelude() -> String
            = "conflict" "s"? " with \"" name:$((!['"'][_])+) "\"" (" " manager_using())? ":" {
                name.to_owned()
            }

        rule fieldpath() -> fieldpath::PathBuf
            = path:$((!['\n'][_])+) {
                fieldpath::parse(path).unwrap()
            }

        rule path_list() -> Vec<fieldpath::PathBuf>
            = paths:("- " path: fieldpath() {path}) ** "\n"

        rule conflict() -> Conflict
            = manager:manager_prelude() paths:(
                "\n" paths:path_list() {paths} /
                " " path:fieldpath() {vec![path]}
            ) {
                Conflict(
                    manager,
                    paths,
                )
            }

        pub rule message() -> Vec<Conflict>
            = total_count:prelude() " " conflicts:conflict() ** "\n" {
                conflicts
            }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use fieldpath::Element::*;
    use fieldpath::PathBuf;
    use serde_json::Value;

    #[test]
    fn parsing() {
        let parsed = conflict_error_parser::message(r#"Apply failed with 2 conflicts: conflicts with "hayasaka.lach.pw/git":
- .spec.volumeClaimTemplates
- .spec.template.spec.containers[name="gitlab-postgresql"].volumeMounts[mountPath="/bitnami/postgresql"].subPath"#).unwrap();
        assert_eq!(
            parsed,
            vec![Conflict(
                "hayasaka.lach.pw/git".to_owned(),
                vec![
                    PathBuf(vec![
                        Field("spec".to_owned()),
                        Field("volumeClaimTemplates".to_owned()),
                    ]),
                    PathBuf(vec![
                        Field("spec".to_owned()),
                        Field("template".to_owned()),
                        Field("spec".to_owned()),
                        Field("containers".to_owned()),
                        Select(
                            "name".to_owned(),
                            Value::String("gitlab-postgresql".to_owned())
                        ),
                        Field("volumeMounts".to_owned()),
                        Select(
                            "mountPath".to_owned(),
                            Value::String("/bitnami/postgresql".to_owned())
                        ),
                        Field("subPath".to_owned()),
                    ]),
                ]
            )]
        );
    }

    #[test]
    fn failed_using() {
        conflict_error_parser::message(r#"Apply failed with 1 conflict: conflict with "kubectl-client-side-apply" using rbac.authorization.k8s.io/v1: .subjects"#).unwrap();
    }

    #[test]
    fn to_string() {
        assert_eq!(PathBuf(vec![
            Field("spec".to_owned()),
            Field("template".to_owned()),
            Field("spec".to_owned()),
            Field("containers".to_owned()),
            Select("name".to_owned(), Value::String("gitlab-postgresql".to_owned())),
            Field("volumeMounts".to_owned()),
            Select("mountPath".to_owned(), Value::String("/bitnami/postgresql".to_owned())),
            Field("subPath".to_owned()),
        ]).to_string(), ".spec.template.spec.containers[name=\"gitlab-postgresql\"].volumeMounts[mountPath=\"/bitnami/postgresql\"].subPath");
    }
}
