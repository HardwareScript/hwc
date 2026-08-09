use super::config::AutoRouter;
use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_engine::geometry::BoundingBox;
use hwc_engine::netlist::NetId;
use rustc_hash::FxHashMap;

pub struct RoutingData {
    pub resolved_routes: Vec<crate::ir::routing::types::ResolvedRoute>,
    pub net_id_to_name: FxHashMap<NetId, CompactString>,
    pub net_layer_targets: FxHashMap<CompactString, i64>,
    pub net_layer_targets_by_id: FxHashMap<NetId, i64>,
    pub net_layer_names_by_id: FxHashMap<NetId, CompactString>, // NEW: Store layer names directly
    pub net_declared_widths: FxHashMap<CompactString, i64>,
    pub net_currents_ma: FxHashMap<CompactString, f64>,
    pub obstacle_bboxes: Vec<BoundingBox>,
    pub net_intents: FxHashMap<CompactString, CompactString>,
}

impl<'a> AutoRouter<'a> {
    pub(crate) fn build_routing_data(&mut self) -> Result<RoutingData, IrError> {
        let mut resolved_routes = Vec::new();
        let mut net_id_to_name = FxHashMap::default();
        let mut net_layer_targets = FxHashMap::default();
        let mut net_layer_targets_by_id = FxHashMap::default();
        let mut net_layer_names_by_id = FxHashMap::default(); // NEW
        let mut net_declared_widths = FxHashMap::default();
        let mut net_currents_ma = FxHashMap::default();
        let mut net_intents = FxHashMap::default();

        if !self.config.auto_routes.is_empty() {
            let auto_routes = self.config.auto_routes.clone();
            let mut used_ports = FxHashMap::default();

            for route in &auto_routes {
                let net_id = self.find_net_id_for_name("TEMP_NET")?;
                let actual_net_id = crate::ir::routing::register_net_for_route(
                    self.space,
                    route,
                    self.symbol_table,
                    self.eval_context,
                    self.stackup_manager,
                    self.profile,
                    None,
                )
                .unwrap_or(net_id);

                let net_name = self
                    .space
                    .netlist
                    .get_net(actual_net_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "unnamed".into());

                let min_width = self.space.fabrication_constraints.as_ref()
                    .map(|c| c.trace.min_width_nm)
                    .ok_or_else(|| IrError::MissingAsicConstraint {
                        message: "Route requires trace width constraint but none are loaded.".to_string(),
                        hint: "Ensure a profile with 'trace:' constraints is declared in the space definition.".into(),
                    })?;

                let mut modified_route = route.clone();
                self.resolve_route_ports(route, &mut modified_route, &mut used_ports, min_width);

                let route_width_nm = if let Some(ref width_expr) = route.width {
                    crate::ir::conversions::evaluate_expression_to_nm(
                        width_expr,
                        self.symbol_table,
                        self.eval_context,
                    )
                    .unwrap_or(min_width)
                } else {
                    min_width
                };

                match crate::ir::routing::resolve_endpoint_entity_ids(&modified_route) {
                    Ok((from_id, to_id)) => {
                        match self.create_resolved_route(
                            actual_net_id,
                            from_id,
                            to_id,
                            route_width_nm,
                            &net_name,
                            &modified_route,
                        ) {
                            Ok(resolved) => {
                                resolved_routes.push(resolved);
                                net_id_to_name.insert(actual_net_id, net_name.clone());

                                if let Some(ref layer_id) = route.layer {
                                    // v0.2.0: Query routing layer database for routing Z elevation
                                    // Use the layer bottom Z (routing elevation) not the centerline
                                    if let Ok(routing_z) =
                                        self.space.routing_layer_db.get_routing_z(&layer_id.name)
                                    {
                                        net_layer_targets.insert(net_name.clone(), routing_z);
                                        net_layer_targets_by_id.insert(actual_net_id, routing_z);
                                        net_layer_names_by_id
                                            .insert(actual_net_id, layer_id.name.clone()); // NEW: Store layer name
                                        eprintln!("[ROUTING BUILDER] Route for net '{}' (id={}) targets layer '{}' at routing Z={}nm", 
                                            net_name, actual_net_id.raw(), layer_id.name, routing_z);
                                    }
                                }
                                if let Some(ref width_expr) = route.width {
                                    if let Ok(w_nm) =
                                        crate::ir::conversions::evaluate_expression_to_nm(
                                            width_expr,
                                            self.symbol_table,
                                            self.eval_context,
                                        )
                                    {
                                        net_declared_widths.insert(net_name.clone(), w_nm);
                                    }
                                }
                                if let Some(ref ac) = route.current_limit_ac {
                                    let rms = crate::ir::conversions::evaluate_expression_to_ma(
                                        &ac.rms,
                                        self.symbol_table,
                                    )
                                    .unwrap_or(0.0);
                                    let peak = crate::ir::conversions::evaluate_expression_to_ma(
                                        &ac.peak,
                                        self.symbol_table,
                                    )
                                    .unwrap_or(rms);
                                    net_currents_ma.insert(net_name.clone(), peak);
                                }
                                if let Some(ref intent_name) = route.intent {
                                    eprintln!("[ROUTING BUILDER] Route from {:?} to {:?} has intent: '{}' for net '{}'", 
                                route.from, route.to, intent_name, net_name);
                                    net_intents.insert(net_name.clone(), intent_name.clone());
                                } else {
                                    eprintln!("[ROUTING BUILDER] Route from {:?} to {:?} has NO intent for net '{}'", 
                                route.from, route.to, net_name);
                                }
                            }
                            Err(e) => {
                                eprintln!("[ROUTER ERROR] Failed to create resolved route for net '{}': {:?}", net_name, e);
                                return Err(e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[ROUTER WARNING] Failed to resolve endpoint EntityIds for route on net '{}': {:?} - skipping", net_name, e);
                    }
                }
            }
        }

        for resolved in &resolved_routes {
            let name = net_id_to_name
                .entry(resolved.net_id)
                .or_insert_with(|| {
                    CompactString::from(format!("chain_net_{}", resolved.net_id.raw()))
                })
                .clone();
            if let Some(net_data) = self.space.netlist.get_net(resolved.net_id) {
                if let Some(c) = net_data.current_ma {
                    net_currents_ma.entry(name).or_insert(c);
                }
            }
        }

        let mut obstacle_bboxes = Vec::new();
        for metadata in self.space.entity_graph.get_component_metadata() {
            obstacle_bboxes.push(metadata.bbox);
        }
        for layer in self.space.entity_graph.get_substrate_layers().iter() {
            if layer.net == hwc_engine::NetId::UNCONNECTED
                && layer.layer_type
                    == hwc_engine::geometry_router::entity_graph::SubstrateLayerType::Pour
            {
                obstacle_bboxes.push(layer.bbox);
            }
        }
        for trace in &self.space.analytic_routes {
            for segment in &trace.segments {
                obstacle_bboxes.push(segment.to_bounding_box(trace.cross_section.width_nm));
            }
        }

        Ok(RoutingData {
            resolved_routes,
            net_id_to_name,
            net_layer_targets,
            net_layer_targets_by_id,
            net_layer_names_by_id, // NEW
            net_declared_widths,
            net_currents_ma,
            obstacle_bboxes,
            net_intents,
        })
    }

    fn resolve_route_ports(
        &self,
        route: &hwc_parser::Route,
        modified_route: &mut hwc_parser::Route,
        used_ports: &mut FxHashMap<
            (CompactString, CompactString),
            Vec<hwc_engine::geometry_router::port_escape::CardinalPort>,
        >,
        min_width: i64,
    ) {
        use hwc_engine::geometry_router::port_escape::CardinalPort;
        let from_key = match &route.from {
            hwc_parser::RouteEndpointSpec::ComponentPin {
                component_name,
                pin_name,
                ..
            } => (component_name.clone(), pin_name.clone()),
            hwc_parser::RouteEndpointSpec::SpaceEntity { name, .. } => {
                (name.clone(), CompactString::default())
            }
        };
        let to_key = match &route.to {
            hwc_parser::RouteEndpointSpec::ComponentPin {
                component_name,
                pin_name,
                ..
            } => (component_name.clone(), pin_name.clone()),
            hwc_parser::RouteEndpointSpec::SpaceEntity { name, .. } => {
                (name.clone(), CompactString::default())
            }
        };

        if modified_route.exit_escape.is_none() {
            if let Ok((start, goal, _, _)) =
                crate::ir::routing::calculate_boundary_points(self.space, route, min_width)
            {
                let dx = goal.x - start.x;
                let dy = goal.y - start.y;
                let mut port = if dy.abs() >= 1_000_000 && dy.abs() > dx.abs() / 4 {
                    if dy > 0 {
                        CardinalPort::North
                    } else {
                        CardinalPort::South
                    }
                } else if dx > 0 {
                    CardinalPort::East
                } else {
                    CardinalPort::West
                };

                if let Some(used) = used_ports.get(&from_key) {
                    if !used.is_empty() {
                        let opposite = match used[0] {
                            CardinalPort::North => CardinalPort::South,
                            CardinalPort::South => CardinalPort::North,
                            CardinalPort::East => CardinalPort::West,
                            CardinalPort::West => CardinalPort::East,
                        };
                        if !used.contains(&opposite) {
                            port = opposite;
                        }
                    }
                    if used.contains(&port) {
                        for p in [
                            CardinalPort::East,
                            CardinalPort::West,
                            CardinalPort::North,
                            CardinalPort::South,
                        ] {
                            if !used.contains(&p) {
                                port = p;
                                break;
                            }
                        }
                    }
                }
                used_ports.entry(from_key).or_default().push(port);
                modified_route.exit_escape = Some(hwc_parser::RouteEscape {
                    port: match port {
                        CardinalPort::North => hwc_parser::CardinalDirection::North,
                        CardinalPort::South => hwc_parser::CardinalDirection::South,
                        CardinalPort::East => hwc_parser::CardinalDirection::East,
                        CardinalPort::West => hwc_parser::CardinalDirection::West,
                    },
                    offset: None,
                    span: route.span,
                });
            }
        }

        if modified_route.enter_escape.is_none() {
            if let Ok((start, goal, _, _)) =
                crate::ir::routing::calculate_boundary_points(self.space, route, min_width)
            {
                let dx = goal.x - start.x;
                let dy = goal.y - start.y;
                let mut port = if dy.abs() >= 1_000_000 && dy.abs() > dx.abs() / 4 {
                    if dy > 0 {
                        CardinalPort::South
                    } else {
                        CardinalPort::North
                    }
                } else if dx > 0 {
                    CardinalPort::West
                } else {
                    CardinalPort::East
                };

                if let Some(used) = used_ports.get(&to_key) {
                    if !used.is_empty() {
                        let opposite = match used[0] {
                            CardinalPort::North => CardinalPort::South,
                            CardinalPort::South => CardinalPort::North,
                            CardinalPort::East => CardinalPort::West,
                            CardinalPort::West => CardinalPort::East,
                        };
                        if !used.contains(&opposite) {
                            port = opposite;
                        }
                    }
                    if used.contains(&port) {
                        for p in [
                            CardinalPort::West,
                            CardinalPort::East,
                            CardinalPort::South,
                            CardinalPort::North,
                        ] {
                            if !used.contains(&p) {
                                port = p;
                                break;
                            }
                        }
                    }
                }
                used_ports.entry(to_key).or_default().push(port);
                modified_route.enter_escape = Some(hwc_parser::RouteEscape {
                    port: match port {
                        CardinalPort::North => hwc_parser::CardinalDirection::North,
                        CardinalPort::South => hwc_parser::CardinalDirection::South,
                        CardinalPort::East => hwc_parser::CardinalDirection::East,
                        CardinalPort::West => hwc_parser::CardinalDirection::West,
                    },
                    offset: None,
                    span: route.span,
                });
            }
        }
    }

    fn create_resolved_route(
        &self,
        net_id: NetId,
        from_id: hwc_engine::EntityId,
        to_id: hwc_engine::EntityId,
        width_nm: i64,
        net_name: &CompactString,
        route: &hwc_parser::Route,
    ) -> Result<crate::ir::routing::types::ResolvedRoute, IrError> {
        let convert_escape =
            |esc: &Option<hwc_parser::RouteEscape>| -> crate::ir::routing::types::EscapeSpec {
                match esc {
                    Some(e) => {
                        let port = match e.port {
                            hwc_parser::CardinalDirection::North => {
                                crate::ir::routing::types::CardinalDirection::North
                            }
                            hwc_parser::CardinalDirection::South => {
                                crate::ir::routing::types::CardinalDirection::South
                            }
                            hwc_parser::CardinalDirection::East => {
                                crate::ir::routing::types::CardinalDirection::East
                            }
                            hwc_parser::CardinalDirection::West => {
                                crate::ir::routing::types::CardinalDirection::West
                            }
                        };
                        let offset = match &e.offset {
                            Some(hwc_parser::EdgeOffsetSpec::Named(
                                hwc_parser::NamedPosition::Top,
                            )) => crate::ir::routing::types::EdgeOffset::Percentage(1.0),
                            Some(hwc_parser::EdgeOffsetSpec::Named(
                                hwc_parser::NamedPosition::Bottom,
                            )) => crate::ir::routing::types::EdgeOffset::Percentage(0.0),
                            Some(hwc_parser::EdgeOffsetSpec::Named(
                                hwc_parser::NamedPosition::Center,
                            )) => crate::ir::routing::types::EdgeOffset::Center,
                            Some(hwc_parser::EdgeOffsetSpec::Percentage(p)) => {
                                crate::ir::routing::types::EdgeOffset::Percentage(*p)
                            }
                            Some(hwc_parser::EdgeOffsetSpec::Measurement(m)) => {
                                crate::ir::routing::types::EdgeOffset::MeasurementNm(*m)
                            }
                            None => crate::ir::routing::types::EdgeOffset::Center,
                        };
                        crate::ir::routing::types::EscapeSpec { port, offset }
                    }
                    None => crate::ir::routing::types::EscapeSpec::default(),
                }
            };

        // v0.2.0 DATABASE-DRIVEN: Layer name is REQUIRED (no fallbacks)
        let layer_name = route
            .layer
            .as_ref()
            .map(|layer_id| layer_id.name.clone())
            .ok_or_else(|| IrError::MissingRouteParameter {
                route: format!("{:?} to {:?}", route.from, route.to).into(),
                parameter: "layer".into(),
                hint: "Every route MUST explicitly declare which layer to use.\n\
                       Example:\n\
                         route A to B:\n\
                           layer: metal1\n\
                           width: 200nm"
                    .into(),
            })?;

        let mut resolved = crate::ir::routing::types::ResolvedRoute::new(
            net_id,
            from_id,
            to_id,
            width_nm,
            net_name.clone(),
            layer_name,
        )
        .with_escapes(
            convert_escape(&route.exit_escape),
            convert_escape(&route.enter_escape),
        );

        // DEPRECATED: target_layer_z override is no longer used
        // Layer Z is now queried from RoutingLayerDatabase during routing
        if let Some(ref layer_id) = route.layer {
            if let Some(target_z) = self.stackup_manager.get_layer_centerline_z(&layer_id.name) {
                resolved = resolved.with_layer_override(target_z);
            }
        }

        Ok(resolved)
    }
}
