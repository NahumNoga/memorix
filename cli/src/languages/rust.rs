use crate::{
    flat_schema::{
        FlatValidatedNamespace, FlatValidatedReferenceTypeItemKind, FlatValidatedSchema,
        FlatValidatedTypeItem, TypeObjectItem,
    },
    parser::{Engine, Value, ALL_CACHE_OPERATIONS, ALL_PUBSUB_OPERATIONS, ALL_TASK_OPERATIONS},
};

fn indent(level: usize) -> String {
    "    ".repeat(level)
}
fn flat_type_item_to_code(
    flat_type_item: &FlatValidatedTypeItem,
    schema: &FlatValidatedSchema,
    namespace_level: usize,
) -> String {
    match flat_type_item {
        FlatValidatedTypeItem::Optional(x) => format!(
            "Option<{}>",
            flat_type_item_to_code(x, schema, namespace_level)
        )
        .to_string(),
        FlatValidatedTypeItem::Array(x) => format!(
            "Vec<{}>",
            flat_type_item_to_code(x, schema, namespace_level)
        )
        .to_string(),
        FlatValidatedTypeItem::Reference(x) => {
            let namespace = x
                .namespace_indexes
                .iter()
                .fold(&schema.global_namespace, |acc, &i| &acc.namespaces[i].1);
            let name = match x.kind {
                FlatValidatedReferenceTypeItemKind::TypeItem(i) => {
                    namespace.flat_type_items[i].0.clone()
                }
                FlatValidatedReferenceTypeItemKind::TypeObjectItem(i) => {
                    namespace.type_object_items[i].0.clone()
                }
                FlatValidatedReferenceTypeItemKind::Enum(i) => namespace.enum_items[i].0.clone(),
            };
            format!(
                "{super}{name}",
                super = "super::".repeat(namespace_level - x.namespace_indexes.len())
            )
        }
        FlatValidatedTypeItem::Boolean => "bool".to_string(),
        FlatValidatedTypeItem::String => "String".to_string(),
        FlatValidatedTypeItem::U32 => "u32".to_string(),
        FlatValidatedTypeItem::U64 => "u64".to_string(),
        FlatValidatedTypeItem::I32 => "i32".to_string(),
        FlatValidatedTypeItem::I64 => "i64".to_string(),
        FlatValidatedTypeItem::F32 => "f32".to_string(),
        FlatValidatedTypeItem::F64 => "f64".to_string(),
    }
}

fn value_to_code(value: &Value) -> String {
    match value {
        Value::String(x) => format!("memorix_client_redis::Value::from_string(\"{x}\")"),
        Value::Env(x) => format!("memorix_client_redis::Value::from_env_variable(\"{x}\")"),
    }
}

fn type_object_item_to_code(
    name: &str,
    type_item_object: &TypeObjectItem,
    schema: &FlatValidatedSchema,
    level: usize,
) -> String {
    let base_indent = indent(level);
    format!(
        r#"
{base_indent}#[memorix_client_redis::serialization]
{base_indent}#[derive(Clone, PartialEq, std::fmt::Debug)]
{base_indent}pub struct {name} {{
{}
{base_indent}}}
"#,
        type_item_object
            .properties
            .iter()
            .map(|(property_name, flat_type_item)| format!(
                "{base_indent}    pub {}: {},",
                match property_name == "type" {
                    true => "r#type",
                    false => property_name,
                },
                flat_type_item_to_code(flat_type_item, schema, level)
            )
            .to_string())
            .collect::<Vec<_>>()
            .join("\n")
    )
    .to_string()
}

fn namespace_to_code(
    namespace: &FlatValidatedNamespace,
    name_tree: Vec<String>,
    schema: &FlatValidatedSchema,
) -> String {
    let mut result = String::from("");
    let base_indent = indent(name_tree.len());

    for (name, values) in &namespace.enum_items {
        result.push_str(&format!(
            r#"
{base_indent}#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
{base_indent}#[memorix_client_redis::serialization]
{base_indent}#[derive(Clone, PartialEq, std::fmt::Debug)]
{base_indent}pub enum {name} {{
{}
}}

"#,
            values
                .iter()
                .map(|x| format!("{base_indent}    {x},"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    for (name, type_item_object) in &namespace.type_object_items {
        result.push_str(&format!(
            "{}\n",
            type_object_item_to_code(name.as_str(), type_item_object, schema, name_tree.len())
        ));
    }
    for (name, flat_type_item) in &namespace.flat_type_items {
        result.push_str(&format!(
            "{base_indent}pub type {name} = {};\n\n",
            flat_type_item_to_code(flat_type_item, schema, name_tree.len())
        ));
    }
    for (name, sub_namespace) in &namespace.namespaces {
        result.push_str(&format!(
            r#"{base_indent}pub mod {name} {{
{namespace_content}
{base_indent}}}
"#,
            name = stringcase::snake_case(name),
            namespace_content = namespace_to_code(
                sub_namespace,
                name_tree
                    .clone()
                    .into_iter()
                    .chain(std::iter::once(name.clone()))
                    .collect(),
                schema,
            ),
        ));
    }
    if !namespace.cache_items.is_empty() {
        result.push_str(&format!(
            r#"{base_indent}#[derive(Clone)]
{base_indent}#[allow(non_snake_case)]
{base_indent}pub struct MemorixCache {{
{struct_content}
{base_indent}}}

{base_indent}impl MemorixCache {{
{base_indent}    fn new(memorix_base: memorix_client_redis::MemorixBase) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {{
{base_indent}        Ok(Self {{
{impl_content}
{base_indent}        }})
{base_indent}    }}
{base_indent}}}

"#,
            struct_content =
                namespace
                    .cache_items
                    .iter()
                    .map(|(name, item)| {
                        let payload = flat_type_item_to_code(&item.payload, schema, name_tree.len());
                        format!(
                        "{base_indent}    pub {name}: memorix_client_redis::MemorixCacheItem{key}{api}>,",
                        key = match &item.key {
                            None => format!("NoKey<{payload}, "),
                            Some(key) => {
                                let key = flat_type_item_to_code(key, schema, name_tree.len());
                                format!("<{key}, {payload}, ")
                            }
                        },
                        api = ALL_CACHE_OPERATIONS.iter().map(|x| match item.expose.contains(x) {
                            true => "memorix_client_redis::Expose",
                            false => "memorix_client_redis::Hide",
                        }).collect::<Vec<_>>().join(", ")
                    )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            impl_content = namespace
                .cache_items
                .iter()
                .map(|(name, item)| {
                    let options = format!(
                        r#"Some(memorix_client_redis::MemorixCacheOptions {{
{content}
{base_indent}                }})"#,
                        content = [
                            format!(
                                "{base_indent}                    ttl_ms: {},",
                                item.ttl_ms.as_ref().map_or("None".to_string(), |x| format!(
                                    "Some({})",
                                    value_to_code(x)
                                ))
                            ),
                            format!(
                                "{base_indent}                    extend_on_get: {},",
                                item.extend_on_get.as_ref().map_or(
                                    "None".to_string(),
                                    |x| format!("Some({})", value_to_code(x))
                                )
                            ),
                        ]
                        .into_iter()
                        .collect::<Vec<_>>()
                        .join("\n")
                    );
                    format!(
                        r#"{base_indent}            {name}: memorix_client_redis::MemorixCacheItem{key}::new(
{base_indent}                memorix_base.clone(),
{base_indent}                "{name}".to_string(),
{base_indent}                {options},
{base_indent}            )?,"#,
                        key = match &item.key {
                            None => "NoKey",
                            Some(_) => "",
                        },
                        options = options,
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !namespace.pubsub_items.is_empty() {
        result.push_str(&format!(
            r#"{base_indent}#[derive(Clone)]
{base_indent}#[allow(non_snake_case)]
{base_indent}pub struct MemorixPubSub {{
{struct_content}
{base_indent}}}

{base_indent}impl MemorixPubSub {{
{base_indent}    fn new(memorix_base: memorix_client_redis::MemorixBase) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {{
{base_indent}        Ok(Self {{
{impl_content}
{base_indent}        }})
{base_indent}    }}
{base_indent}}}

"#,
            struct_content =
                namespace
                    .pubsub_items
                    .iter()
                    .map(|(name, item)| {
                        let payload = flat_type_item_to_code(&item.payload, schema, name_tree.len());
                        format!(
                        "{base_indent}    pub {name}: memorix_client_redis::MemorixPubSubItem{key}{api}>,",
                        key = match &item.key {
                            None => format!("NoKey<{payload}, "),
                            Some(key) => {
                                let key = flat_type_item_to_code(key, schema, name_tree.len());
                                format!("<{key}, {payload}, ")
                            }
                        },
                        api = ALL_PUBSUB_OPERATIONS.iter().map(|x| match item.expose.contains(x) {
                            true => "memorix_client_redis::Expose",
                            false => "memorix_client_redis::Hide",
                        }).collect::<Vec<_>>().join(", ")
                    )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            impl_content = namespace
                .pubsub_items
                .iter()
                .map(|(name, item)| {
                    format!(
                        r#"{base_indent}            {name}: memorix_client_redis::MemorixPubSubItem{key}::new(
{base_indent}                memorix_base.clone(),
{base_indent}                "{name}".to_string(),
{base_indent}            )?,"#,
                        key = match &item.key {
                            None => "NoKey",
                            Some(_) => "",
                        },
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !namespace.task_items.is_empty() {
        result.push_str(&format!(
            r#"{base_indent}#[derive(Clone)]
{base_indent}#[allow(non_snake_case)]
{base_indent}pub struct MemorixTask {{
{struct_content}
{base_indent}}}

{base_indent}impl MemorixTask {{
{base_indent}    fn new(memorix_base: memorix_client_redis::MemorixBase) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {{
{base_indent}        Ok(Self {{
{impl_content}
{base_indent}        }})
{base_indent}    }}
{base_indent}}}

"#,
            struct_content =
                namespace
                    .task_items
                    .iter()
                    .map(|(name, item)| {
                        let payload = flat_type_item_to_code(&item.payload, schema, name_tree.len());
                        format!(
                        "{base_indent}    pub {name}: memorix_client_redis::MemorixTaskItem{key}{api}>,",
                        key = match &item.key {
                            None => format!("NoKey<{payload}, "),
                            Some(key) => {
                                let key = flat_type_item_to_code(key, schema, name_tree.len());
                                format!("<{key}, {payload}, ")
                            }
                        },
                        api = ALL_TASK_OPERATIONS.iter().map(|x| match item.expose.contains(x) {
                            true => "memorix_client_redis::Expose",
                            false => "memorix_client_redis::Hide",
                        }).collect::<Vec<_>>().join(", ")
                    )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            impl_content = namespace
                .task_items
                .iter()
                .map(|(name, item)| {
                    let options = format!(
                        r#"Some(memorix_client_redis::MemorixTaskOptions {{
{content}
{base_indent}                }})"#,
                        content =
                            [format!(
                                "{base_indent}                    queue_type: {},",
                                item.queue_type.as_ref().map_or(
                                    "None".to_string(),
                                    |x| format!("Some({})", value_to_code(x))
                                )
                            ),]
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                    format!(
                        r#"{base_indent}            {name}: memorix_client_redis::MemorixTaskItem{key}::new(
{base_indent}                memorix_base.clone(),
                "{name}".to_string(),
{base_indent}                {options},
{base_indent}            )?,"#,
                        key = match &item.key {
                            None => "NoKey",
                            Some(_) => "",
                        },
                        options = options,
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    result.push_str(&format!(
        r#"{base_indent}#[derive(Clone)]
{base_indent}#[allow(non_snake_case)]
{base_indent}pub struct Memorix {{
{struct_content}
{base_indent}}}

{base_indent}const MEMORIX_NAMESPACE_NAME_TREE: &[&str] = &[{name_tree}];

{base_indent}impl Memorix {{
{impl_new}
{base_indent}        Ok(Self {{
{impl_content}
{base_indent}        }})
{base_indent}    }}
{base_indent}}}
"#,
        name_tree = name_tree
            .iter()
            .map(|x| format!("\"{}\"", x))
            .collect::<Vec<_>>()
            .join(", "),
        struct_content = [
            namespace
                .namespaces
                .iter()
                .map(|(name, _)| format!(
                    "{base_indent}    pub {name}: {namespace_name}::Memorix,",
                    namespace_name = stringcase::snake_case(name)
                ))
                .collect::<Vec<_>>()
                .join("\n"),
            [
                (!namespace.cache_items.is_empty())
                    .then(|| format!("{base_indent}    pub cache: MemorixCache,")),
                (!namespace.pubsub_items.is_empty())
                    .then(|| format!("{base_indent}    pub pubsub: MemorixPubSub,")),
                (!namespace.task_items.is_empty())
                    .then(|| format!("{base_indent}    pub task: MemorixTask,")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n")
        ]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n"),
        impl_new = match name_tree.is_empty() {
            true => format!(r#"{base_indent}    pub async fn new() -> Result<Memorix, Box<dyn std::error::Error + Sync + Send>> {{
{base_indent}        let _memorix_base = memorix_client_redis::MemorixBase::new(
{base_indent}            {redis_url},
{base_indent}            MEMORIX_NAMESPACE_NAME_TREE,
{base_indent}        )
{base_indent}        .await?;"#, redis_url= match &schema.engine {
            Engine::Redis(x) => format!("&{}", value_to_code(x)),
            }),
            false => format!(
                r#"{base_indent}    pub fn new(
{base_indent}        other: memorix_client_redis::MemorixBase,
{base_indent}    ) -> Result<Memorix, Box<dyn std::error::Error + Sync + Send>> {{
{base_indent}        let _memorix_base = memorix_client_redis::MemorixBase::from(
{base_indent}            other,
{base_indent}            MEMORIX_NAMESPACE_NAME_TREE,
{base_indent}        );"#),
        },
        impl_content = [
            namespace
                .namespaces
                .iter()
                .map(|(name, _)| format!(
                    "{base_indent}            {name}: {namespace_name}::Memorix::new(_memorix_base.clone())?,",
                    namespace_name = stringcase::snake_case(name),
                ))
                .collect::<Vec<_>>()
                .join("\n"),
            [
                (!namespace.cache_items.is_empty()).then(|| format!(
                    "{base_indent}            cache: MemorixCache::new(_memorix_base.clone())?,"
                )),
                (!namespace.pubsub_items.is_empty()).then(|| format!(
                    "{base_indent}            pubsub: MemorixPubSub::new(_memorix_base.clone())?,"
                )),
                (!namespace.task_items.is_empty()).then(|| format!(
                    "{base_indent}            task: MemorixTask::new(_memorix_base.clone())?,"
                )),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n")
        ]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
    ));
    result
}

pub fn codegen(schema: &FlatValidatedSchema) -> String {
    format!(
        r#"#![allow(dead_code)]
extern crate memorix_client_redis;

{}"#,
        namespace_to_code(&schema.global_namespace, vec![], schema,)
    )
    .to_string()
}
