use compact_str::CompactString;
use rustc_hash::FxHashMap;

use crate::geometry::EntityId;
use crate::material::MaterialId;

/// Parameters for registering a via connection
pub struct ViaRegistrationParams<'a> {
    pub entity_name: &'a str,
    pub bottom_layer: &'a str,
    pub bottom_z: i64,
    pub top_layer: &'a str,
    pub top_z: i64,
    pub position_2d: (i64, i64),
    pub bottom_material: MaterialId,
    pub top_material: MaterialId,
}

/// Exact connection point for routing — the single source of truth for where
/// a via or pour connects to a routing layer.
#[derive(Debug, Clone)]
pub struct RoutingConnectionPoint {
    /// Entity that owns this connection (via, pour, pad)
    pub entity_id: EntityId,
    /// Layer name this point connects to (e.g., "metal1")
    pub layer_name: CompactString,
    /// Exact Z elevation of the connection surface in nanometers
    pub z_elevation: i64,
    /// XY center position in nanometers
    pub position_2d: (i64, i64),
    /// The material of the layer this connection reaches
    pub material_id: MaterialId,
    /// Which surface of a via this represents
    pub connection_type: ConnectionType,
}

/// Which surface of a via is providing the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    /// Bottom of a via — connects to the layer BELOW the via body
    ViaBottom,
    /// Top of a via — connects to the layer ABOVE the via body
    ViaTop,
    /// A pour/contact surface on a single Z plane
    PourSurface,
    /// A pad surface
    PadSurface,
}

/// Errors from the layer connection database.
#[derive(Debug, Clone)]
pub enum LayerConnectionError {
    /// No connection registered for this entity on the requested layer
    NoConnectionPoint {
        entity: CompactString,
        layer: CompactString,
    },
    /// Entity has no connections at all
    EntityNotRegistered { entity: CompactString },
    /// Connection Z doesn't match the routing layer Z (compiler bug indicator)
    ConnectionZMismatch {
        entity: CompactString,
        connection_z: i64,
        expected_routing_z: i64,
        layer: CompactString,
    },
    /// Entity already has a connection on this layer
    DuplicateConnection {
        entity: CompactString,
        layer: CompactString,
    },
}

impl std::fmt::Display for LayerConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoConnectionPoint { entity, layer } => {
                write!(
                    f,
                    "Entity '{}' has no connection point on layer '{}'",
                    entity, layer
                )
            }
            Self::EntityNotRegistered { entity } => {
                write!(
                    f,
                    "Entity '{}' is not registered in the connection database",
                    entity
                )
            }
            Self::ConnectionZMismatch {
                entity,
                connection_z,
                expected_routing_z,
                layer,
            } => {
                write!(
                    f,
                    "Entity '{}' connection Z={}nm doesn't match routing layer '{}' Z={}nm",
                    entity, connection_z, layer, expected_routing_z
                )
            }
            Self::DuplicateConnection { entity, layer } => {
                write!(
                    f,
                    "Entity '{}' already has a connection registered on layer '{}'",
                    entity, layer
                )
            }
        }
    }
}

impl std::error::Error for LayerConnectionError {}

/// Database of all routing connection points — single source of truth for
/// where entities connect to routing layers.
///
/// Populated during placement when vias and contacts are placed.
/// Queried during routing to determine exact connection Z coordinates.
///
/// v0.2.2: Uses composite key (entity_name, layer_name, position) for physical
/// identity instead of name-only key. This prevents false collisions when
/// multiple vias share a name (e.g., template interpolation failures or
/// auto-generated contacts).
#[derive(Debug, Clone)]
pub struct LayerConnectionDatabase {
    /// Connection points indexed by (entity name, layer name, XY position)
    /// v0.2.2: Position added to key for physical identity
    connections: FxHashMap<(CompactString, CompactString, (i64, i64)), RoutingConnectionPoint>,
    /// All layers an entity connects to (for lookup by entity name)
    entity_layers: FxHashMap<CompactString, Vec<CompactString>>,
}

impl LayerConnectionDatabase {
    /// Create an empty database.
    pub fn new() -> Self {
        Self {
            connections: FxHashMap::default(),
            entity_layers: FxHashMap::default(),
        }
    }

    /// Register a via's connection points (called during contact placement).
    ///
    /// Registers connection points on BOTH the bottom and top layers.
    /// The `bottom_connection_z` should be the top of the bottom layer
    /// (where the via meets the lower routing layer).
    /// The `top_connection_z` should be the bottom of the top layer
    /// (where the via meets the upper routing layer).
    ///
    /// v0.2.2: Uses (entity_name, layer_name, position) as key for physical
    /// identity. Multiple vias with the same name at different positions are
    /// distinct physical objects and register successfully.
    pub fn register_via(
        &mut self,
        params: ViaRegistrationParams,
    ) -> Result<(), LayerConnectionError> {
        let entity: CompactString = params.entity_name.into();
        let pos = params.position_2d;

        // Register bottom connection point
        // v0.2.2: Include position in key for physical identity
        let bottom_key = (entity.clone(), params.bottom_layer.into(), pos);
        if self.connections.contains_key(&bottom_key) {
            return Err(LayerConnectionError::DuplicateConnection {
                entity: entity.clone(),
                layer: params.bottom_layer.into(),
            });
        }
        self.connections.insert(
            bottom_key,
            RoutingConnectionPoint {
                entity_id: EntityId::from_semantic(params.entity_name),
                layer_name: params.bottom_layer.into(),
                z_elevation: params.bottom_z,
                position_2d: params.position_2d,
                material_id: params.bottom_material,
                connection_type: ConnectionType::ViaBottom,
            },
        );

        // Register top connection point
        // v0.2.2: Include position in key for physical identity
        let top_key = (entity.clone(), params.top_layer.into(), pos);
        if self.connections.contains_key(&top_key) {
            return Err(LayerConnectionError::DuplicateConnection {
                entity: entity.clone(),
                layer: params.top_layer.into(),
            });
        }
        self.connections.insert(
            top_key,
            RoutingConnectionPoint {
                entity_id: EntityId::from_semantic(params.entity_name),
                layer_name: params.top_layer.into(),
                z_elevation: params.top_z,
                position_2d: params.position_2d,
                material_id: params.top_material,
                connection_type: ConnectionType::ViaTop,
            },
        );

        // Track which layers this entity connects to
        let layers = self.entity_layers.entry(entity).or_default();
        layers.push(params.bottom_layer.into());
        layers.push(params.top_layer.into());

        Ok(())
    }

    /// Register a pour or pad surface connection (single Z plane).
    ///
    /// v0.2.2: Uses (entity_name, layer_name, position) as key for physical identity.
    pub fn register_surface(
        &mut self,
        entity_name: &str,
        layer_name: &str,
        z_elevation: i64,
        position_2d: (i64, i64),
        material_id: MaterialId,
        connection_type: ConnectionType,
    ) -> Result<(), LayerConnectionError> {
        let entity: CompactString = entity_name.into();
        let layer: CompactString = layer_name.into();
        // v0.2.2: Include position in key for physical identity
        let key = (entity.clone(), layer.clone(), position_2d);

        if self.connections.contains_key(&key) {
            return Err(LayerConnectionError::DuplicateConnection { entity, layer });
        }

        self.connections.insert(
            key,
            RoutingConnectionPoint {
                entity_id: EntityId::from_semantic(entity_name),
                layer_name: layer.clone(),
                z_elevation,
                position_2d,
                material_id,
                connection_type,
            },
        );

        self.entity_layers.entry(entity).or_default().push(layer);

        Ok(())
    }

    /// Get the connection point for an entity on a specific layer.
    ///
    /// Returns `Err` if no connection exists — never falls back to guessing.
    ///
    /// v0.2.2: If multiple connections exist (same entity+layer, different positions),
    /// returns the first one found. For precise lookup, use `get_connection_at_position`.
    pub fn get_connection_point(
        &self,
        entity_name: &str,
        layer_name: &str,
    ) -> Result<&RoutingConnectionPoint, LayerConnectionError> {
        // v0.2.2: Search for any connection matching entity+layer (position varies)
        let entity: CompactString = entity_name.into();
        let layer: CompactString = layer_name.into();
        
        self.connections
            .iter()
            .find(|((e, l, _pos), _conn)| e == &entity && l == &layer)
            .map(|(_key, conn)| conn)
            .ok_or_else(|| LayerConnectionError::NoConnectionPoint {
                entity: entity_name.into(),
                layer: layer_name.into(),
            })
    }

    /// Get the connection point for an entity on a specific layer at a specific position.
    ///
    /// v0.2.2: Precise lookup by (entity, layer, position) triple.
    pub fn get_connection_at_position(
        &self,
        entity_name: &str,
        layer_name: &str,
        position: (i64, i64),
    ) -> Result<&RoutingConnectionPoint, LayerConnectionError> {
        let key = (entity_name.into(), layer_name.into(), position);
        self.connections
            .get(&key)
            .ok_or_else(|| LayerConnectionError::NoConnectionPoint {
                entity: entity_name.into(),
                layer: layer_name.into(),
            })
    }

    /// Get all connection points for an entity on a specific layer.
    ///
    /// v0.2.2: Returns all connections (multiple positions may exist).
    pub fn get_all_connections_on_layer(
        &self,
        entity_name: &str,
        layer_name: &str,
    ) -> Vec<&RoutingConnectionPoint> {
        let entity: CompactString = entity_name.into();
        let layer: CompactString = layer_name.into();
        
        self.connections
            .iter()
            .filter(|((e, l, _pos), _conn)| e == &entity && l == &layer)
            .map(|(_key, conn)| conn)
            .collect()
    }

    /// Get all connection points for an entity (across all layers).
    pub fn get_entity_connections(&self, entity_name: &str) -> Option<&[CompactString]> {
        self.entity_layers.get(entity_name).map(|v| v.as_slice())
    }

    /// Check if an entity has a connection on a specific layer.
    ///
    /// v0.2.2: Returns true if ANY connection exists for entity+layer (any position).
    pub fn has_connection(&self, entity_name: &str, layer_name: &str) -> bool {
        let entity: CompactString = entity_name.into();
        let layer: CompactString = layer_name.into();
        
        self.connections
            .keys()
            .any(|(e, l, _pos)| e == &entity && l == &layer)
    }

    /// Get all registered entity names.
    pub fn registered_entities(&self) -> impl Iterator<Item = &CompactString> {
        self.entity_layers.keys()
    }

    /// Validate that all via connections are compatible with their routing layers.
    ///
    /// v0.2.0: Data-driven validation using stackup information.
    /// - For ROUTABLE layers: via connection Z must match the routing Z (strict check)
    /// - For NON-ROUTABLE layers: via connections at interfaces are valid (no check needed)
    ///
    /// v0.2.2: Updated to work with position-based keys.
    ///
    /// Call this before routing to catch via-layer mismatches early.
    pub fn validate(
        &self,
        routing_z_map: &FxHashMap<CompactString, i64>,
        stackup: &[crate::space::StackupLayer],
    ) -> Result<(), Vec<LayerConnectionError>> {
        let mut errors = Vec::new();

        // Build a map of layer name -> is_routable from the stackup
        let routable_map: FxHashMap<&str, bool> = stackup
            .iter()
            .map(|layer| (layer.name.as_str(), layer.is_routable))
            .collect();

        for ((_entity, layer, _pos), conn) in &self.connections {
            // Only validate routable layers - non-routable layers connect at interfaces
            let is_routable = routable_map.get(layer.as_str()).copied().unwrap_or(false);

            if !is_routable {
                // Non-routable layers (like "active") connect at interfaces, not routing Z
                // This is expected and correct - skip validation
                continue;
            }

            // v0.2.1 FIX: Vias connect at layer INTERFACES, pours connect at ROUTING surfaces
            // Only validate routing_z for PourSurface and PadSurface connections
            match conn.connection_type {
                ConnectionType::ViaBottom | ConnectionType::ViaTop => {
                    // Vias connect at layer interfaces (top of lower layer, bottom of upper layer)
                    // These are NOT at routing_z, so skip routing_z validation for vias
                    // Instead, validate that via connections are within layer bounds
                    if let Some(layer_info) = stackup.iter().find(|l| l.name == *layer) {
                        if conn.z_elevation < layer_info.z_bottom
                            || conn.z_elevation > layer_info.z_top
                        {
                            errors.push(LayerConnectionError::ConnectionZMismatch {
                                entity: conn.entity_id.to_string().into(),
                                connection_z: conn.z_elevation,
                                expected_routing_z: layer_info.z_bottom, // Use z_bottom as reference in error message
                                layer: layer.clone(),
                            });
                        }
                    }
                }
                ConnectionType::PourSurface | ConnectionType::PadSurface => {
                    // Pours and pads MUST connect at routing_z for proper routing
                    if let Some(&expected_z) = routing_z_map.get(layer) {
                        if conn.z_elevation != expected_z {
                            errors.push(LayerConnectionError::ConnectionZMismatch {
                                entity: conn.entity_id.to_string().into(),
                                connection_z: conn.z_elevation,
                                expected_routing_z: expected_z,
                                layer: layer.clone(),
                            });
                        }
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Get the total number of registered connections.
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }
}

impl Default for LayerConnectionDatabase {
    fn default() -> Self {
        Self::new()
    }
}
