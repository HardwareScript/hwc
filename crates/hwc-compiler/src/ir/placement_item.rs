/// Placement item in the unified statement stream (v0.1.7)
///
/// This enum is used by the topological sort and placement pipeline.
#[derive(Debug, Clone)]
pub enum PlacementItem {
    Substrate(hwc_parser::SubstratePlacement),
    Component(Box<hwc_parser::ComponentPlacement>),
    Pour(hwc_parser::PourPlacement),
    Plane(Box<hwc_parser::PlanePlacement>),
    Contact(hwc_parser::ContactPlacement),
    Route(hwc_parser::Route),
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
            PlacementItem::Contact(c) => c
                .name
                .as_ref()
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("__contact_{}", index).into()),
            PlacementItem::Route(_) => format!("__route_{}", index).into(),
        }
    }
}
