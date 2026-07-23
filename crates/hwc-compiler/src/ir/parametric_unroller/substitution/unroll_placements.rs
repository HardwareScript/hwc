use super::expression_sub::{substitute_in_coordinate, substitute_in_expression};
use super::name_sub::{
    substitute_in_component_name, substitute_in_net_binding, substitute_in_net_name,
};
use crate::ir::errors::IrError;
use hwc_parser::{ComponentPlacement, ContactPlacement, PlanePlacement, PourPlacement};

pub fn unroll_component(
    component: &ComponentPlacement,
    variable: &str,
    value: usize,
) -> Result<ComponentPlacement, IrError> {
    let name = component
        .name
        .as_ref()
        .map(|n| substitute_in_component_name(n, variable, value));

    let position = component
        .position
        .as_ref()
        .map(|p| substitute_in_coordinate(p, variable, value))
        .transpose()?;

    let pin_net_bindings = component
        .pin_net_bindings
        .iter()
        .map(|(pin, binding)| {
            let substituted_binding = substitute_in_net_binding(binding, variable, value)?;
            Ok((pin.clone(), substituted_binding))
        })
        .collect::<Result<rustc_hash::FxHashMap<_, _>, IrError>>()?;

    let elevation = if let Some(elevation) = &component.elevation {
        match elevation {
            hwc_parser::Elevation::Physical { start, end } => {
                Some(hwc_parser::Elevation::Physical {
                    start: substitute_in_expression(start, variable, value)?,
                    end: end
                        .as_ref()
                        .map(|e| substitute_in_expression(e, variable, value))
                        .transpose()?,
                })
            }
            hwc_parser::Elevation::Semantic(id) => {
                Some(hwc_parser::Elevation::Semantic(id.clone()))
            }
            hwc_parser::Elevation::Relative => Some(hwc_parser::Elevation::Relative),
        }
    } else {
        None
    };

    Ok(ComponentPlacement {
        component_type: component.component_type.clone(),
        parameters: component.parameters.clone(),
        name,
        position,
        rotation: component.rotation.clone(),
        elevation,
        mount: component.mount,
        standoff: component.standoff.clone(),
        array_config: component.array_config.clone(),
        pin_net_bindings,
        waivers: component.waivers.clone(),
        relational_constraints: component.relational_constraints.clone(),
        span: component.span,
    })
}

pub fn unroll_pour(
    pour: &PourPlacement,
    variable: &str,
    value: usize,
) -> Result<PourPlacement, IrError> {
    let name = substitute_in_component_name(&pour.name, variable, value);

    let net = pour
        .net
        .as_ref()
        .map(|n| substitute_in_net_name(n, variable, value))
        .transpose()?;

    let boundary = if let Some(b) = &pour.boundary {
        match b {
            hwc_parser::PourBoundary::Rect(from, to) => {
                let from_sub = substitute_in_coordinate(from, variable, value)?;
                let to_sub = substitute_in_coordinate(to, variable, value)?;
                Some(hwc_parser::PourBoundary::Rect(
                    Box::new(from_sub),
                    Box::new(to_sub),
                ))
            }
            hwc_parser::PourBoundary::Circle { center, radius } => {
                let center_sub = substitute_in_coordinate(center, variable, value)?;
                let radius_sub = substitute_in_expression(radius, variable, value)?;
                Some(hwc_parser::PourBoundary::Circle {
                    center: Box::new(center_sub),
                    radius: radius_sub,
                })
            }
        }
    } else {
        None
    };

    let elevation = match &pour.elevation {
        hwc_parser::Elevation::Physical { start, end } => hwc_parser::Elevation::Physical {
            start: substitute_in_expression(start, variable, value)?,
            end: end
                .as_ref()
                .map(|e| substitute_in_expression(e, variable, value))
                .transpose()?,
        },
        hwc_parser::Elevation::Semantic(id) => hwc_parser::Elevation::Semantic(id.clone()),
        hwc_parser::Elevation::Relative => hwc_parser::Elevation::Relative,
    };

    let thickness = pour
        .thickness
        .as_ref()
        .map(|t| substitute_in_expression(t, variable, value))
        .transpose()?;

    Ok(PourPlacement {
        material: pour.material.clone(),
        name,
        elevation,
        thickness,
        boundary,
        net,
        device: pour.device.clone(),
        thermal_relief: pour.thermal_relief,
        waivers: pour.waivers.clone(),
        relational_constraints: pour.relational_constraints.clone(),
        inside_region: pour.inside_region.clone(),
        span: pour.span,
    })
}

pub fn unroll_plane(
    plane: &PlanePlacement,
    variable: &str,
    value: usize,
) -> Result<PlanePlacement, IrError> {
    let name = substitute_in_component_name(&plane.name, variable, value);

    let net = plane
        .net
        .as_ref()
        .map(|n| substitute_in_net_name(n, variable, value))
        .transpose()?;

    let from = plane
        .from
        .as_ref()
        .map(|c| substitute_in_coordinate(c, variable, value))
        .transpose()?;

    let to = plane
        .to
        .as_ref()
        .map(|c| substitute_in_coordinate(c, variable, value))
        .transpose()?;

    let elevation = match &plane.elevation {
        hwc_parser::Elevation::Physical { start, end } => hwc_parser::Elevation::Physical {
            start: substitute_in_expression(start, variable, value)?,
            end: end
                .as_ref()
                .map(|e| substitute_in_expression(e, variable, value))
                .transpose()?,
        },
        hwc_parser::Elevation::Semantic(id) => hwc_parser::Elevation::Semantic(id.clone()),
        hwc_parser::Elevation::Relative => hwc_parser::Elevation::Relative,
    };

    let thickness = plane
        .thickness
        .as_ref()
        .map(|t| substitute_in_expression(t, variable, value))
        .transpose()?;

    let cutouts = plane
        .cutouts
        .iter()
        .map(|cutout| match cutout {
            hwc_parser::CutoutShape::Rectangle { width, height, at } => {
                let width_sub = substitute_in_expression(width, variable, value)?;
                let height_sub = substitute_in_expression(height, variable, value)?;
                let at_sub = substitute_in_coordinate(at, variable, value)?;
                Ok(hwc_parser::CutoutShape::Rectangle {
                    width: width_sub,
                    height: height_sub,
                    at: at_sub,
                })
            }
            hwc_parser::CutoutShape::Circle { radius, at } => {
                let radius_sub = substitute_in_expression(radius, variable, value)?;
                let at_sub = substitute_in_coordinate(at, variable, value)?;
                Ok(hwc_parser::CutoutShape::Circle {
                    radius: radius_sub,
                    at: at_sub,
                })
            }
        })
        .collect::<Result<Vec<_>, IrError>>()?;

    Ok(PlanePlacement {
        material: plane.material.clone(),
        name,
        shape: plane.shape.clone(),
        elevation,
        thickness,
        from,
        to,
        net,
        cutouts,
        relational_constraints: plane.relational_constraints.clone(),
        inside_region: plane.inside_region.clone(),
        span: plane.span,
    })
}

pub fn unroll_contact(
    contact: &ContactPlacement,
    variable: &str,
    value: usize,
) -> Result<ContactPlacement, IrError> {
    let name = contact
        .name
        .as_ref()
        .map(|n| substitute_in_component_name(n, variable, value));

    let net = contact
        .net
        .as_ref()
        .map(|n| substitute_in_net_name(n, variable, value))
        .transpose()?;

    let position = substitute_in_coordinate(&contact.position, variable, value)?;

    let from_elevation = match &contact.from_elevation {
        hwc_parser::Elevation::Physical { start, end } => hwc_parser::Elevation::Physical {
            start: substitute_in_expression(start, variable, value)?,
            end: end
                .as_ref()
                .map(|e| substitute_in_expression(e, variable, value))
                .transpose()?,
        },
        hwc_parser::Elevation::Semantic(id) => hwc_parser::Elevation::Semantic(id.clone()),
        hwc_parser::Elevation::Relative => hwc_parser::Elevation::Relative,
    };
    let to_elevation = match &contact.to_elevation {
        hwc_parser::Elevation::Physical { start, end } => hwc_parser::Elevation::Physical {
            start: substitute_in_expression(start, variable, value)?,
            end: end
                .as_ref()
                .map(|e| substitute_in_expression(e, variable, value))
                .transpose()?,
        },
        hwc_parser::Elevation::Semantic(id) => hwc_parser::Elevation::Semantic(id.clone()),
        hwc_parser::Elevation::Relative => hwc_parser::Elevation::Relative,
    };

    let mut properties = rustc_hash::FxHashMap::default();
    for (name, expr) in &contact.properties {
        properties.insert(
            name.clone(),
            substitute_in_expression(expr, variable, value)?,
        );
    }

    Ok(ContactPlacement {
        material: contact.material.clone(),
        name,
        position,
        from_elevation,
        to_elevation,
        net,
        properties,
        contour: contact.contour.clone(),
        span: contact.span,
    })
}
