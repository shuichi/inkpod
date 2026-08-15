use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;

use super::schema::InkScriptCommandSchema;
use super::types::{InkScriptTypedValue, InkScriptTypedValueKind};

const MAX_CATALOG_EXPRESSION_DEPTH: usize = 64;
#[cfg(test)]
const MAX_PORTABILITY_RULES: usize = 256;

#[allow(
    dead_code,
    reason = "catalog entries are supplied by their feature owners"
)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum InkScriptPortabilityClass {
    Portable,
    RequiresBinding,
    StrictSourceOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InkScriptPortability {
    pub(crate) class: InkScriptPortabilityClass,
    pub(crate) required_preconditions: Vec<&'static str>,
}

#[allow(
    dead_code,
    reason = "the closed catalog AST precedes its feature entries"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CatalogNumericExpression {
    Literal(u64),
    Field(Vec<&'static str>),
    ListLength {
        path: Vec<&'static str>,
        maximum: u64,
    },
    CheckedAdd(Box<Self>, Box<Self>),
    CheckedSubtract(Box<Self>, Box<Self>),
    CheckedMultiply(Box<Self>, Box<Self>),
    CeilDivideNonzero(Box<Self>, Box<Self>),
    Min(Box<Self>, Box<Self>),
    Max(Box<Self>, Box<Self>),
    CheckedAbs(Box<Self>),
    BoundedSum {
        path: Vec<&'static str>,
        maximum_items: u64,
        body: Box<Self>,
    },
    Conditional {
        condition: Box<CatalogBooleanExpression>,
        when_true: Box<Self>,
        when_false: Box<Self>,
    },
}

#[allow(
    dead_code,
    reason = "catalog v1 comparison nodes are populated by owner entries"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CatalogComparison {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[allow(
    dead_code,
    reason = "catalog v1 predicate nodes are populated by owner entries"
)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CatalogBooleanExpression {
    Literal(bool),
    Compare {
        comparison: CatalogComparison,
        left: CatalogNumericExpression,
        right: CatalogNumericExpression,
    },
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
    Not(Box<Self>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CatalogPortabilityEvaluator {
    pub(crate) rules: Vec<(CatalogBooleanExpression, InkScriptPortability)>,
    pub(crate) default: InkScriptPortability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CatalogWorkFormula {
    pub(crate) max_invocations: CatalogNumericExpression,
    pub(crate) max_output_ids: CatalogNumericExpression,
    pub(crate) max_asset_bytes: CatalogNumericExpression,
    pub(crate) max_work_units: CatalogNumericExpression,
    pub(crate) max_output_growth: CatalogNumericExpression,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CatalogWorkEstimate {
    pub(crate) max_invocations: u64,
    pub(crate) max_output_ids: u64,
    pub(crate) max_asset_bytes: u64,
    pub(crate) max_work_units: u64,
    pub(crate) max_output_growth: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CatalogResultMetadata {
    pub(crate) name: &'static str,
    pub(crate) namespace: Option<&'static str>,
    pub(crate) owner_role: Option<&'static str>,
    pub(crate) output_id_ordinal: Option<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CatalogAssetMetadata {
    pub(crate) name: &'static str,
    pub(crate) kind: &'static str,
    pub(crate) inline: bool,
    pub(crate) external: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CatalogEditorMetadata {
    pub(crate) family: &'static str,
    pub(crate) legacy_projection: Option<&'static str>,
    pub(crate) allow_skip_dependents: bool,
}

#[allow(
    dead_code,
    reason = "non-mutation variants are required for the rejection gate"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CatalogCommandDomain {
    DocumentMutation,
    Query,
    View,
    Session,
}

#[derive(Clone, Debug)]
pub(crate) struct CatalogEntry {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) schema: InkScriptCommandSchema,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) domain: CatalogCommandDomain,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) results: Vec<CatalogResultMetadata>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) assets: Vec<CatalogAssetMetadata>,
    pub(crate) portability: CatalogPortabilityEvaluator,
    pub(crate) work: CatalogWorkFormula,
    pub(crate) editor: CatalogEditorMetadata,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CatalogError {
    InvalidEntry,
    #[cfg_attr(not(test), allow(dead_code))]
    DuplicateCommand,
    #[cfg_attr(not(test), allow(dead_code))]
    NonMutationCommand,
    UnknownField,
    TypeMismatch,
    Overflow,
    ZeroDivisor,
    ResourceLimit,
}

#[derive(Clone, Debug)]
pub(crate) struct InkScriptCatalogView {
    entries: BTreeMap<&'static str, CatalogEntry>,
}

impl InkScriptCatalogView {
    #[cfg(test)]
    pub(crate) fn test_only(entries: Vec<CatalogEntry>) -> Result<Self, CatalogError> {
        let mut by_name = BTreeMap::new();
        for entry in entries {
            validate_entry(&entry)?;
            if entry.domain != CatalogCommandDomain::DocumentMutation {
                return Err(CatalogError::NonMutationCommand);
            }
            if by_name.insert(entry.schema.name, entry).is_some() {
                return Err(CatalogError::DuplicateCommand);
            }
        }
        Ok(Self { entries: by_name })
    }

    pub(crate) fn entry(&self, name: &str) -> Result<&CatalogEntry, CatalogError> {
        self.entries.get(name).ok_or(CatalogError::InvalidEntry)
    }

    pub(crate) fn evaluate_portability(
        &self,
        name: &str,
        arguments: &InkScriptTypedValue,
    ) -> Result<InkScriptPortability, CatalogError> {
        let entry = self.entry(name)?;
        for (condition, result) in &entry.portability.rules {
            if evaluate_boolean(condition, arguments, 0)? {
                return Ok(result.clone());
            }
        }
        Ok(entry.portability.default.clone())
    }

    pub(crate) fn evaluate_work(
        &self,
        name: &str,
        arguments: &InkScriptTypedValue,
    ) -> Result<CatalogWorkEstimate, CatalogError> {
        let entry = self.entry(name)?;
        Ok(CatalogWorkEstimate {
            max_invocations: evaluate_output(&entry.work.max_invocations, arguments)?,
            max_output_ids: evaluate_output(&entry.work.max_output_ids, arguments)?,
            max_asset_bytes: evaluate_output(&entry.work.max_asset_bytes, arguments)?,
            max_work_units: evaluate_output(&entry.work.max_work_units, arguments)?,
            max_output_growth: evaluate_output(&entry.work.max_output_growth, arguments)?,
        })
    }
}

#[cfg(test)]
fn validate_entry(entry: &CatalogEntry) -> Result<(), CatalogError> {
    if entry.portability.rules.len() > MAX_PORTABILITY_RULES
        || entry.results.len() != entry.schema.results.len()
    {
        return Err(CatalogError::InvalidEntry);
    }
    let schema_results = entry
        .schema
        .results
        .iter()
        .map(|result| result.name)
        .collect::<BTreeSet<_>>();
    if entry
        .results
        .iter()
        .any(|result| !schema_results.contains(result.name))
    {
        return Err(CatalogError::InvalidEntry);
    }
    let mut ordinals = BTreeSet::new();
    if entry.results.iter().any(|result| {
        result
            .output_id_ordinal
            .is_some_and(|ordinal| !ordinals.insert(ordinal))
    }) {
        return Err(CatalogError::InvalidEntry);
    }
    let mut asset_names = BTreeSet::new();
    if entry.assets.iter().any(|asset| {
        asset.name.is_empty()
            || asset.kind.is_empty()
            || (!asset.inline && !asset.external)
            || !asset_names.insert(asset.name)
    }) || entry.editor.family.is_empty()
    {
        return Err(CatalogError::InvalidEntry);
    }
    Ok(())
}

fn evaluate_boolean(
    expression: &CatalogBooleanExpression,
    arguments: &InkScriptTypedValue,
    depth: usize,
) -> Result<bool, CatalogError> {
    if depth >= MAX_CATALOG_EXPRESSION_DEPTH {
        return Err(CatalogError::ResourceLimit);
    }
    match expression {
        CatalogBooleanExpression::Literal(value) => Ok(*value),
        CatalogBooleanExpression::Compare {
            comparison,
            left,
            right,
        } => {
            let left = evaluate_numeric(left, arguments, depth + 1)?;
            let right = evaluate_numeric(right, arguments, depth + 1)?;
            Ok(match comparison {
                CatalogComparison::Equal => left == right,
                CatalogComparison::NotEqual => left != right,
                CatalogComparison::Less => left < right,
                CatalogComparison::LessEqual => left <= right,
                CatalogComparison::Greater => left > right,
                CatalogComparison::GreaterEqual => left >= right,
            })
        }
        CatalogBooleanExpression::And(left, right) => {
            Ok(evaluate_boolean(left, arguments, depth + 1)?
                && evaluate_boolean(right, arguments, depth + 1)?)
        }
        CatalogBooleanExpression::Or(left, right) => {
            Ok(evaluate_boolean(left, arguments, depth + 1)?
                || evaluate_boolean(right, arguments, depth + 1)?)
        }
        CatalogBooleanExpression::Not(value) => Ok(!evaluate_boolean(value, arguments, depth + 1)?),
    }
}

fn evaluate_output(
    expression: &CatalogNumericExpression,
    arguments: &InkScriptTypedValue,
) -> Result<u64, CatalogError> {
    u64::try_from(evaluate_numeric(expression, arguments, 0)?).map_err(|_| CatalogError::Overflow)
}

fn evaluate_numeric(
    expression: &CatalogNumericExpression,
    arguments: &InkScriptTypedValue,
    depth: usize,
) -> Result<i128, CatalogError> {
    if depth >= MAX_CATALOG_EXPRESSION_DEPTH {
        return Err(CatalogError::ResourceLimit);
    }
    let binary = |left: &CatalogNumericExpression,
                  right: &CatalogNumericExpression|
     -> Result<(i128, i128), CatalogError> {
        Ok((
            evaluate_numeric(left, arguments, depth + 1)?,
            evaluate_numeric(right, arguments, depth + 1)?,
        ))
    };
    match expression {
        CatalogNumericExpression::Literal(value) => Ok(i128::from(*value)),
        CatalogNumericExpression::Field(path) => numeric_value(value_at_path(arguments, path)?),
        CatalogNumericExpression::ListLength { path, maximum } => {
            let InkScriptTypedValueKind::List(values) = value_at_path(arguments, path)?.kind()
            else {
                return Err(CatalogError::TypeMismatch);
            };
            let length = u64::try_from(values.len()).map_err(|_| CatalogError::Overflow)?;
            (length <= *maximum)
                .then_some(i128::from(length))
                .ok_or(CatalogError::ResourceLimit)
        }
        CatalogNumericExpression::CheckedAdd(left, right) => {
            let (left, right) = binary(left, right)?;
            left.checked_add(right).ok_or(CatalogError::Overflow)
        }
        CatalogNumericExpression::CheckedSubtract(left, right) => {
            let (left, right) = binary(left, right)?;
            left.checked_sub(right).ok_or(CatalogError::Overflow)
        }
        CatalogNumericExpression::CheckedMultiply(left, right) => {
            let (left, right) = binary(left, right)?;
            left.checked_mul(right).ok_or(CatalogError::Overflow)
        }
        CatalogNumericExpression::CeilDivideNonzero(left, right) => {
            let (left, right) = binary(left, right)?;
            if left < 0 || right <= 0 {
                return Err(if right == 0 {
                    CatalogError::ZeroDivisor
                } else {
                    CatalogError::TypeMismatch
                });
            }
            let adjusted = left
                .checked_add(right.checked_sub(1).ok_or(CatalogError::ZeroDivisor)?)
                .ok_or(CatalogError::Overflow)?;
            adjusted.checked_div(right).ok_or(CatalogError::ZeroDivisor)
        }
        CatalogNumericExpression::Min(left, right) => {
            let (left, right) = binary(left, right)?;
            Ok(left.min(right))
        }
        CatalogNumericExpression::Max(left, right) => {
            let (left, right) = binary(left, right)?;
            Ok(left.max(right))
        }
        CatalogNumericExpression::CheckedAbs(value) => {
            evaluate_numeric(value, arguments, depth + 1)?
                .checked_abs()
                .ok_or(CatalogError::Overflow)
        }
        CatalogNumericExpression::BoundedSum {
            path,
            maximum_items,
            body,
        } => {
            let InkScriptTypedValueKind::List(values) = value_at_path(arguments, path)?.kind()
            else {
                return Err(CatalogError::TypeMismatch);
            };
            let length = u64::try_from(values.len()).map_err(|_| CatalogError::Overflow)?;
            if length > *maximum_items {
                return Err(CatalogError::ResourceLimit);
            }
            let mut total = 0i128;
            for value in values {
                total = total
                    .checked_add(evaluate_numeric(body, value, depth + 1)?)
                    .ok_or(CatalogError::Overflow)?;
            }
            Ok(total)
        }
        CatalogNumericExpression::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            if evaluate_boolean(condition, arguments, depth + 1)? {
                evaluate_numeric(when_true, arguments, depth + 1)
            } else {
                evaluate_numeric(when_false, arguments, depth + 1)
            }
        }
    }
}

fn value_at_path<'a>(
    root: &'a InkScriptTypedValue,
    path: &[&str],
) -> Result<&'a InkScriptTypedValue, CatalogError> {
    let mut value = root;
    for segment in path {
        let InkScriptTypedValueKind::Record(fields) = value.kind() else {
            return Err(CatalogError::TypeMismatch);
        };
        value = fields.get(*segment).ok_or(CatalogError::UnknownField)?;
    }
    Ok(value)
}

fn numeric_value(value: &InkScriptTypedValue) -> Result<i128, CatalogError> {
    match value.kind() {
        InkScriptTypedValueKind::U32(value) => Ok(i128::from(*value)),
        InkScriptTypedValueKind::U64(value) => Ok(i128::from(*value)),
        InkScriptTypedValueKind::I32(value) => Ok(i128::from(*value)),
        InkScriptTypedValueKind::I64(value) | InkScriptTypedValueKind::Q16(value) => {
            Ok(i128::from(*value))
        }
        _ => Err(CatalogError::TypeMismatch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inkscript::schema::{
        InkScriptCommandResultSchema, InkScriptFieldSchema, InkScriptResultAvailability,
    };

    const FIELDS: &[InkScriptFieldSchema] = &[
        InkScriptFieldSchema::required("count", "u32", 0),
        InkScriptFieldSchema::required("values", "list<u32>", 1),
    ];
    const RESULTS: &[InkScriptCommandResultSchema] = &[InkScriptCommandResultSchema::ordered_list(
        "layers",
        "layer_ref",
        InkScriptResultAvailability::AlwaysOnSuccess,
        0,
    )];

    fn arguments(count: u32) -> InkScriptTypedValue {
        InkScriptTypedValue::new(
            "catalog_test_invocation",
            InkScriptTypedValueKind::Record(BTreeMap::from([
                (
                    "count".to_owned(),
                    InkScriptTypedValue::new("u32", InkScriptTypedValueKind::U32(count)),
                ),
                (
                    "values".to_owned(),
                    InkScriptTypedValue::new(
                        "list<u32>",
                        InkScriptTypedValueKind::List(vec![
                            InkScriptTypedValue::new("u32", InkScriptTypedValueKind::U32(2)),
                            InkScriptTypedValue::new("u32", InkScriptTypedValueKind::U32(3)),
                        ]),
                    ),
                ),
            ])),
        )
    }

    fn entry(domain: CatalogCommandDomain) -> CatalogEntry {
        CatalogEntry {
            schema: InkScriptCommandSchema::with_results("catalog_test", FIELDS, RESULTS),
            domain,
            results: vec![CatalogResultMetadata {
                name: "layers",
                namespace: Some("document_stable"),
                owner_role: None,
                output_id_ordinal: Some(0),
            }],
            assets: vec![CatalogAssetMetadata {
                name: "payload",
                kind: "canonical_raster",
                inline: true,
                external: true,
            }],
            portability: CatalogPortabilityEvaluator {
                rules: vec![(
                    CatalogBooleanExpression::Compare {
                        comparison: CatalogComparison::Greater,
                        left: CatalogNumericExpression::Field(vec!["count"]),
                        right: CatalogNumericExpression::Literal(1),
                    },
                    InkScriptPortability {
                        class: InkScriptPortabilityClass::RequiresBinding,
                        required_preconditions: vec!["semantic_target"],
                    },
                )],
                default: InkScriptPortability {
                    class: InkScriptPortabilityClass::Portable,
                    required_preconditions: Vec::new(),
                },
            },
            work: CatalogWorkFormula {
                max_invocations: CatalogNumericExpression::Literal(1),
                max_output_ids: CatalogNumericExpression::Field(vec!["count"]),
                max_asset_bytes: CatalogNumericExpression::Literal(0),
                max_work_units: CatalogNumericExpression::BoundedSum {
                    path: vec!["values"],
                    maximum_items: 2,
                    body: Box::new(CatalogNumericExpression::Field(Vec::new())),
                },
                max_output_growth: CatalogNumericExpression::ListLength {
                    path: vec!["values"],
                    maximum: 2,
                },
            },
            editor: CatalogEditorMetadata {
                family: "test",
                legacy_projection: None,
                allow_skip_dependents: true,
            },
        }
    }

    #[test]
    fn catalog_metadata_portability_and_work_are_closed_and_checked() {
        let view =
            InkScriptCatalogView::test_only(vec![entry(CatalogCommandDomain::DocumentMutation)])
                .unwrap();
        let catalog_entry = view.entry("catalog_test").unwrap();
        assert_eq!(catalog_entry.results[0].output_id_ordinal, Some(0));
        assert_eq!(catalog_entry.assets[0].kind, "canonical_raster");
        assert!(catalog_entry.editor.allow_skip_dependents);
        assert_eq!(catalog_entry.schema.results[0].name, "layers");

        assert_eq!(
            view.evaluate_portability("catalog_test", &arguments(2))
                .unwrap()
                .class,
            InkScriptPortabilityClass::RequiresBinding
        );
        assert_eq!(
            view.evaluate_portability("catalog_test", &arguments(1))
                .unwrap()
                .class,
            InkScriptPortabilityClass::Portable
        );
        assert_eq!(
            view.evaluate_work("catalog_test", &arguments(2)).unwrap(),
            CatalogWorkEstimate {
                max_invocations: 1,
                max_output_ids: 2,
                max_asset_bytes: 0,
                max_work_units: 5,
                max_output_growth: 2,
            }
        );
    }

    #[test]
    fn query_view_session_and_invalid_formula_paths_fail_closed() {
        for domain in [
            CatalogCommandDomain::Query,
            CatalogCommandDomain::View,
            CatalogCommandDomain::Session,
        ] {
            assert_eq!(
                InkScriptCatalogView::test_only(vec![entry(domain)]).unwrap_err(),
                CatalogError::NonMutationCommand
            );
        }

        let view =
            InkScriptCatalogView::test_only(vec![entry(CatalogCommandDomain::DocumentMutation)])
                .unwrap();
        let invalid = CatalogNumericExpression::CheckedAdd(
            Box::new(CatalogNumericExpression::Literal(u64::MAX)),
            Box::new(CatalogNumericExpression::Literal(1)),
        );
        assert_eq!(
            evaluate_output(&invalid, &arguments(1)).unwrap_err(),
            CatalogError::Overflow
        );
        assert_eq!(
            evaluate_output(
                &CatalogNumericExpression::Field(vec!["missing"]),
                &arguments(1)
            )
            .unwrap_err(),
            CatalogError::UnknownField
        );
        assert_eq!(
            evaluate_output(
                &CatalogNumericExpression::ListLength {
                    path: vec!["values"],
                    maximum: 1,
                },
                &arguments(1)
            )
            .unwrap_err(),
            CatalogError::ResourceLimit
        );
        assert_eq!(
            evaluate_output(
                &CatalogNumericExpression::CeilDivideNonzero(
                    Box::new(CatalogNumericExpression::Literal(1)),
                    Box::new(CatalogNumericExpression::Literal(0)),
                ),
                &arguments(1)
            )
            .unwrap_err(),
            CatalogError::ZeroDivisor
        );

        let short_circuit = CatalogBooleanExpression::Or(
            Box::new(CatalogBooleanExpression::Literal(true)),
            Box::new(CatalogBooleanExpression::Compare {
                comparison: CatalogComparison::Equal,
                left: CatalogNumericExpression::Field(vec!["missing"]),
                right: CatalogNumericExpression::Literal(0),
            }),
        );
        assert!(evaluate_boolean(&short_circuit, &arguments(1), 0).unwrap());
        assert_eq!(
            view.entry("missing").unwrap_err(),
            CatalogError::InvalidEntry
        );
    }
}
