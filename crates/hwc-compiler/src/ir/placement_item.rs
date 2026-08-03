/// Placement item in the unified statement stream (v0.1.7, v0.2.0+)
///
/// This enum is used by the topological sort and placement pipeline.
#[derive(Debug, Clone)]
pub enum PlacementItem {
    Substrate(hwc_parser::SubstratePlacement),
    Component(Box<hwc_parser::ComponentPlacement>),
    Pour(hwc_parser::PourPlacement),
    Plane(Box<hwc_parser::PlanePlacement>),
    Contact(hwc_parser::ContactPlacement),
    SpaceInstance(Box<hwc_parser::SpaceInstancePlacement>), // v0.2.1: Hierarchical space composition
    Route(hwc_parser::Route),
    Region(hwc_parser::RegionDefinition), // v0.2.0: Region floorplanning
}

/// v0.2.1: Contextual placement item with associated evaluation context
///
/// Each placement item carries its own evaluation context, which includes:
/// - Space-level let bindings
/// - Loop-scoped let bindings (for items inside for loops)
/// - Loop iteration variables
///
/// This ensures that expressions in coordinates, shapes, etc. are evaluated
/// with the correct variable bindings from their lexical scope.
#[derive(Debug, Clone)]
pub struct ContextualPlacementItem {
    pub item: PlacementItem,
    pub eval_context: hwc_parser::EvaluationContext,
}

impl ContextualPlacementItem {
    pub fn item_id(&self, index: usize) -> compact_str::CompactString {
        self.item.item_id(index)
    }

    pub fn inner(&self) -> &PlacementItem {
        &self.item
    }
}

impl PlacementItem {
    pub fn item_id(&self, index: usize) -> compact_str::CompactString {
        match self {
            PlacementItem::Substrate(_) => format!("__substrate_{}", index).into(),
            PlacementItem::Component(c) => c
                .name
                .as_ref()
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("__comp_{}", index).into()),
            PlacementItem::Pour(p) => p.name.to_string(),
            PlacementItem::Plane(p) => p.name.to_string(),
            PlacementItem::Contact(c) => c.name.base.clone(),
            PlacementItem::SpaceInstance(si) => si.instance_name.base.clone(), // v0.2.1: Space instance name
            PlacementItem::Route(_) => format!("__route_{}", index).into(),
            PlacementItem::Region(r) => r.name.to_string().into(), // v0.2.0: Region name
        }
    }
}
