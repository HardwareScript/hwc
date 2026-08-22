use crate::ir::errors::IrError;
use crate::ir::placement_item::{ContextualPlacementItem, PlacementItem};
use crate::SymbolTable;
use hwc_parser::ast::arena::AstArena;
use hwc_parser::SpaceTopLevelStatement;

/// Push a placement item, assigning it the next dense `item_index`.
///
/// The index doubles as the item's node ID in the dependency graph, so the
/// topological sort and the placement loop are pure integer work.
#[inline]
fn push(items: &mut Vec<ContextualPlacementItem>, item: PlacementItem) {
    items.push(ContextualPlacementItem {
        item,
        item_index: items.len(),
    });
}

/// Collect all placement items from space statements, unrolling for-loops inline
/// while preserving textual order. Unrolled nodes are allocated directly into the
/// mutable `arena` and referenced by their ID — no cloning or boxing.
///
/// v0.2.x: Returns 8-byte `Copy` handles. Nothing is cloned per item: no
/// `EvaluationContext` clones, no key string allocations. Loop variables are
/// already substituted into each unrolled node's fields by the unroller.
pub fn collect_placement_items(
    statements: &[SpaceTopLevelStatement],
    symbol_table: &SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    arena: &mut AstArena,
) -> Result<Vec<ContextualPlacementItem>, IrError> {
    let mut placement_items = Vec::with_capacity(statements.len());

    // v0.2.0: Collect regions FIRST so they can be used as anchors.
    for statement in statements.iter() {
        if let SpaceTopLevelStatement::Region(region_id) = statement {
            push(&mut placement_items, PlacementItem::Region(*region_id));
        }
    }

    for statement in statements.iter() {
        match statement {
            SpaceTopLevelStatement::Substrate(substrate_id) => {
                push(
                    &mut placement_items,
                    PlacementItem::Substrate(*substrate_id),
                );
            }
            SpaceTopLevelStatement::Component(comp_id) => {
                push(&mut placement_items, PlacementItem::Component(*comp_id));
            }
            SpaceTopLevelStatement::Pour(pour_id) => {
                push(&mut placement_items, PlacementItem::Pour(*pour_id));
            }
            SpaceTopLevelStatement::Plane(plane_id) => {
                push(&mut placement_items, PlacementItem::Plane(*plane_id));
            }
            SpaceTopLevelStatement::Contact(contact_id) => {
                push(&mut placement_items, PlacementItem::Contact(*contact_id));
            }
            SpaceTopLevelStatement::SpaceInstance(space_inst_id) => {
                push(
                    &mut placement_items,
                    PlacementItem::SpaceInstance(*space_inst_id),
                );
            }
            SpaceTopLevelStatement::ForLoop(for_loop_id) => {
                let unrolled = crate::ir::parametric_unroller::unroll_for_loop(
                    *for_loop_id,
                    symbol_table,
                    eval_context,
                    arena,
                )?;

                for id in unrolled.components {
                    push(&mut placement_items, PlacementItem::Component(id));
                }
                for id in unrolled.pours {
                    push(&mut placement_items, PlacementItem::Pour(id));
                }
                for id in unrolled.planes {
                    push(&mut placement_items, PlacementItem::Plane(id));
                }
                for id in unrolled.contacts {
                    push(&mut placement_items, PlacementItem::Contact(id));
                }
                for id in unrolled.space_instances {
                    push(&mut placement_items, PlacementItem::SpaceInstance(id));
                }
                for id in unrolled.routes {
                    push(&mut placement_items, PlacementItem::Route(id));
                }
            }
            SpaceTopLevelStatement::Route(route_id) => {
                push(&mut placement_items, PlacementItem::Route(*route_id));
            }
            SpaceTopLevelStatement::Region(_)
            | SpaceTopLevelStatement::Polygon(_)
            | SpaceTopLevelStatement::Expose(_)
            | SpaceTopLevelStatement::RouteNetPolicy(_)
            | SpaceTopLevelStatement::Let(_)
            | SpaceTopLevelStatement::Const(_)
            | SpaceTopLevelStatement::DeviceInstance(_) => {
                // Regions were collected above; the rest is metadata already
                // folded into `eval_context` — nothing to place.
            }
        }
    }

    Ok(placement_items)
}
