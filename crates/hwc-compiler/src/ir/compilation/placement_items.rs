use crate::ir::placement_item::PlacementItem;
use crate::SymbolTable;

/// Collect all placement items from space statements, unrolling for-loops inline
/// while preserving textual order. (v0.2.0+: Regions collected first for anchoring)
pub fn collect_placement_items(
    space_def: &hwc_parser::SpaceDefinition,
    symbol_table: &SymbolTable,
) -> Result<Vec<PlacementItem>, crate::ir::errors::IrError> {
    let mut placement_items = Vec::new();

    // v0.2.0: Collect regions FIRST so they can be used as anchors
    for statement in space_def.statements.iter() {
        if let hwc_parser::SpaceTopLevelStatement::Region(region) = statement {
            placement_items.push(PlacementItem::Region(region.clone()));
        }
    }

    // Then collect other placement items
    for statement in space_def.statements.iter() {
        match statement {
            hwc_parser::SpaceTopLevelStatement::Substrate(sub) => {
                placement_items.push(PlacementItem::Substrate(sub.clone()));
            }
            hwc_parser::SpaceTopLevelStatement::Component(comp) => {
                placement_items.push(PlacementItem::Component(Box::new((**comp).clone())));
            }
            hwc_parser::SpaceTopLevelStatement::Pour(pour) => {
                placement_items.push(PlacementItem::Pour((**pour).clone()));
            }
            hwc_parser::SpaceTopLevelStatement::Plane(plane) => {
                placement_items.push(PlacementItem::Plane(Box::new((**plane).clone())));
            }
            hwc_parser::SpaceTopLevelStatement::Contact(contact) => {
                placement_items.push(PlacementItem::Contact(contact.clone()));
            }
            hwc_parser::SpaceTopLevelStatement::ForLoop(for_loop) => {
                let unrolled =
                    crate::ir::parametric_unroller::unroll_for_loop(for_loop, symbol_table)?;

                for comp in unrolled.components {
                    placement_items.push(PlacementItem::Component(Box::new(comp)));
                }
                for pour in unrolled.pours {
                    placement_items.push(PlacementItem::Pour(pour));
                }
                for plane in unrolled.planes {
                    placement_items.push(PlacementItem::Plane(Box::new(plane)));
                }
                for contact in unrolled.contacts {
                    placement_items.push(PlacementItem::Contact(contact));
                }
                for route in unrolled.routes {
                    placement_items.push(PlacementItem::Route(route));
                }
            }
            hwc_parser::SpaceTopLevelStatement::Route(route) => {
                placement_items.push(PlacementItem::Route(route.clone()));
            }
            hwc_parser::SpaceTopLevelStatement::Polygon(_)
            | hwc_parser::SpaceTopLevelStatement::Expose(_)
            | hwc_parser::SpaceTopLevelStatement::RouteNetPolicy(_)
            | hwc_parser::SpaceTopLevelStatement::Region(_)
            | hwc_parser::SpaceTopLevelStatement::Let(_) => {
                // Already processed regions above; Let bindings are handled in eval_context; others are metadata
            }
        }
    }

    Ok(placement_items)
}
