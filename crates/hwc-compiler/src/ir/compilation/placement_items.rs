use crate::ir::placement_item::{ContextualPlacementItem, PlacementItem};
use crate::SymbolTable;

/// Collect all placement items from space statements, unrolling for-loops inline
/// while preserving textual order. (v0.2.0+: Regions collected first for anchoring)
/// v0.2.1: Returns ContextualPlacementItem with associated evaluation context
pub fn collect_placement_items(
    space_def: &hwc_parser::SpaceDefinition,
    symbol_table: &SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
) -> Result<Vec<ContextualPlacementItem>, crate::ir::errors::IrError> {
    let mut placement_items = Vec::new();

    // v0.2.0: Collect regions FIRST so they can be used as anchors
    for statement in space_def.statements.iter() {
        if let hwc_parser::SpaceTopLevelStatement::Region(region) = statement {
            placement_items.push(ContextualPlacementItem {
                item: PlacementItem::Region(region.clone()),
                eval_context: eval_context.clone(),
            });
        }
    }

    // Then collect other placement items
    for statement in space_def.statements.iter() {
        match statement {
            hwc_parser::SpaceTopLevelStatement::Substrate(sub) => {
                placement_items.push(ContextualPlacementItem {
                    item: PlacementItem::Substrate(sub.clone()),
                    eval_context: eval_context.clone(),
                });
            }
            hwc_parser::SpaceTopLevelStatement::Component(comp) => {
                placement_items.push(ContextualPlacementItem {
                    item: PlacementItem::Component(Box::new((**comp).clone())),
                    eval_context: eval_context.clone(),
                });
            }
            hwc_parser::SpaceTopLevelStatement::Pour(pour) => {
                placement_items.push(ContextualPlacementItem {
                    item: PlacementItem::Pour((**pour).clone()),
                    eval_context: eval_context.clone(),
                });
            }
            hwc_parser::SpaceTopLevelStatement::Plane(plane) => {
                placement_items.push(ContextualPlacementItem {
                    item: PlacementItem::Plane(Box::new((**plane).clone())),
                    eval_context: eval_context.clone(),
                });
            }
            hwc_parser::SpaceTopLevelStatement::Contact(contact) => {
                placement_items.push(ContextualPlacementItem {
                    item: PlacementItem::Contact((*contact).clone()),
                    eval_context: eval_context.clone(),
                });
            }
            hwc_parser::SpaceTopLevelStatement::SpaceInstance(space_inst) => {
                // v0.2.1: Hierarchical space composition
                placement_items.push(ContextualPlacementItem {
                    item: PlacementItem::SpaceInstance(Box::new((**space_inst).clone())),
                    eval_context: eval_context.clone(),
                });
            }
            hwc_parser::SpaceTopLevelStatement::ForLoop(for_loop) => {
                let unrolled = crate::ir::parametric_unroller::unroll_for_loop(
                    for_loop,
                    symbol_table,
                    eval_context,
                )?;

                for contextual_comp in unrolled.components {
                    placement_items.push(ContextualPlacementItem {
                        item: PlacementItem::Component(Box::new(contextual_comp.item)),
                        eval_context: contextual_comp.eval_context,
                    });
                }
                for contextual_pour in unrolled.pours {
                    placement_items.push(ContextualPlacementItem {
                        item: PlacementItem::Pour(contextual_pour.item),
                        eval_context: contextual_pour.eval_context,
                    });
                }
                for contextual_plane in unrolled.planes {
                    placement_items.push(ContextualPlacementItem {
                        item: PlacementItem::Plane(Box::new(contextual_plane.item)),
                        eval_context: contextual_plane.eval_context,
                    });
                }
                for contextual_contact in unrolled.contacts {
                    placement_items.push(ContextualPlacementItem {
                        item: PlacementItem::Contact(contextual_contact.item),
                        eval_context: contextual_contact.eval_context,
                    });
                }
                for contextual_space_inst in unrolled.space_instances {
                    // v0.2.1: Space instances from for-loops
                    placement_items.push(ContextualPlacementItem {
                        item: PlacementItem::SpaceInstance(Box::new(contextual_space_inst.item)),
                        eval_context: contextual_space_inst.eval_context,
                    });
                }
                for contextual_route in unrolled.routes {
                    placement_items.push(ContextualPlacementItem {
                        item: PlacementItem::Route(contextual_route.item),
                        eval_context: contextual_route.eval_context,
                    });
                }
            }
            hwc_parser::SpaceTopLevelStatement::Route(route) => {
                placement_items.push(ContextualPlacementItem {
                    item: PlacementItem::Route(route.clone()),
                    eval_context: eval_context.clone(),
                });
            }
            hwc_parser::SpaceTopLevelStatement::Polygon(_)
            | hwc_parser::SpaceTopLevelStatement::Expose(_)
            | hwc_parser::SpaceTopLevelStatement::RouteNetPolicy(_)
            | hwc_parser::SpaceTopLevelStatement::Region(_)
            | hwc_parser::SpaceTopLevelStatement::Let(_)
            | hwc_parser::SpaceTopLevelStatement::Const(_) => {
                // Already processed regions above; Let/Const bindings are handled in eval_context; others are metadata
            }
        }
    }

    Ok(placement_items)
}
