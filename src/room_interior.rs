use std::cmp::{max, min};
use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::TryInto;

use mcprogedit::block::Block;
use mcprogedit::colour::Colour;
use mcprogedit::coordinates::BlockCoord;
use mcprogedit::positioning::{Direction, Surface2, Surface4, Surface5};
use mcprogedit::world_excerpt::WorldExcerpt;

use image::GrayImage;
use log::warn;
use rand::{Rng, thread_rng};


// What is the shape of the room?
//////////////////////////////////

#[derive(Clone, Copy, Debug)]
pub enum ColumnKind {
    OutOfBounds, // Not within the editable area
    Wall, // Solid wall
    Window, // Wall with 1 m window starting 1 m above floor level
    Door, // Wall with door on floor level
    Floor, // Open area inside room TODO add height to ceiling as well?
}

/// 2D structural map of the room.
#[derive(Clone, Debug)]
pub struct RoomShape {
    columns: Vec<ColumnKind>,
    x_dim: usize,
    z_dim: usize,
    // TODO Add max height (y level below ceiling) as well?
}

impl RoomShape {
    /// Returns a new RoomShape of the given dimensions, with all columns marked out-of-bounds.
    pub fn new((x_dim, z_dim): (usize, usize)) -> Self {
        Self::new_filled((x_dim, z_dim), ColumnKind::OutOfBounds)
    }

    /// Returns a new RoomShape of the given dimensions, with all columns set to `column_kind`.
    pub fn new_filled(
        (x_dim, z_dim): (usize, usize),
        column_kind: ColumnKind,
    ) -> Self {
        let columns_len = x_dim * z_dim;
        let columns = vec![column_kind; columns_len];

        Self { columns, x_dim, z_dim }
    }

    /// Get the dimensions of this RoomShape, as `(x_dimension, z_dimension)`.
    pub fn dimensions(&self) -> (usize, usize) {
        (self.x_dim, self.z_dim)
    }

    /// Set the column kind at the (x, z) location `coordinates` to the given column kind.
    pub fn set_column_kind_at(
        &mut self,
        coordinates: (usize, usize),
        column_kind: ColumnKind,
    ) {
        if let Some(index) = self.index(coordinates) {
            self.columns[index] = column_kind;
        }
    }

    /// Get the column kind at the (x, z) location `coordinates`.
    pub fn column_kind_at(&self, coordinates: (usize, usize)) -> Option<ColumnKind> {
        self.index(coordinates).map(|index| *self.columns.get(index).unwrap())
    }

    fn index(&self, (x, z): (usize, usize)) -> Option<usize> {
        if x >= self.x_dim || z >= self.z_dim {
            None
        } else {
            Some(x + self.x_dim * z)
        }
    }
}


// Where can new objects go?
/////////////////////////////

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PlacementOption {
    OnWall(Surface4), // Registered surface is facing the wall
    OnFloorFreestanding,
    OnFloorBacked(Surface4), // Registered surface is facing wall or object
    FromCeilingFreestanding,
    FromCeilingBacked(Surface4), // Registered surface is facing wall or object
    OnTopSurfaceBacked(Surface4), // Registered surface is facing wall or object
    OnTopSurfaceFreestanding,
    OnSideSurface(Surface4), // Registered surface is facing the neighbouring object
}

type PlacementOptionCollection = HashSet<PlacementOption>;

#[derive(Clone, Debug)]
enum InteriorPlacementState {
    Available(PlacementOptionCollection), // Position is available for any object placement.
    KeepOpen(PlacementOptionCollection), // Position is available for non-blocking objects only.
    OccupiedBlocking, // There's an object there which blocks movement.
    OccupiedOpen, // There's an object there which does not block movement.
}

impl InteriorPlacementState {
    fn is_open(&self) -> bool {
        if let Self::Available(_) | Self::KeepOpen(_) | Self::OccupiedOpen = self {
            true
        } else {
            false
        }
    }
}

type InteriorPlacementStateMap = HashMap<(usize, usize, usize), InteriorPlacementState>;

fn interior_placement_state_map_from_room_shape(room_shape: &RoomShape) -> InteriorPlacementStateMap {
    let mut output = HashMap::new();

    let (x_len, z_len) = room_shape.dimensions();
    let height = 2; // TODO get actual ceiling heights from the room_shape instead, in below for loops

    for x in 0..x_len {
        for z in 0..z_len {
            if let Some(ColumnKind::Floor) = room_shape.column_kind_at((x, z)) {
                for height in 0..height {
                    let mut available_placements = PlacementOptionCollection::new();
                    let neighbourhood_coordinates = neighbourhood_4((x, z));
                    let mut must_be_kept_open = false;

                    if height == 0 {
                        // Ground level
                        for neighbour_coordinates in neighbourhood_coordinates {
                            match room_shape.column_kind_at(neighbour_coordinates) {
                                Some(ColumnKind::Wall)
                                | Some(ColumnKind::Window) => {
                                    let direction = neighbour_direction((x, z), neighbour_coordinates);
                                    available_placements.insert(PlacementOption::OnFloorBacked(direction));
                                }
                                Some(ColumnKind::Door) => must_be_kept_open = true,
                                _ => (),
                            }
                        }
                        if available_placements.is_empty() {
                            available_placements.insert(PlacementOption::OnFloorFreestanding);
                        }
                    } else {
                        // Window level
                        for neighbour_coordinates in neighbourhood_coordinates {
                            match room_shape.column_kind_at(neighbour_coordinates) {
                                Some(ColumnKind::Wall) => {
                                    let direction = neighbour_direction((x, z), neighbour_coordinates);
                                    available_placements.insert(PlacementOption::OnWall(direction));
                                }
                                Some(ColumnKind::Window)
                                | Some(ColumnKind::Door) => must_be_kept_open = true,
                                _ => (),
                            }
                        }
                    }
                    // TODO intermediate levels (above windows)
                    //      * Add OnWall(surface) for each wall neighbour
                    //      * NB No windows or doors on those levels! (At least not yet)
                    // TODO highest (next to ceiling) level
                    //      * Add FromCeilingBacked(surface) for each wall neighbour
                    //      * Add FromCeilingFreestanding if nothing yet

                    let interior_placement_state = if must_be_kept_open {
                        InteriorPlacementState::KeepOpen(available_placements)
                    } else {
                        InteriorPlacementState::Available(available_placements)
                    };
                    output.insert((x, height, z), interior_placement_state);
                }
            }
        }
    }

    output
}


// Internal functions
//////////////////////

/// Checks if obstructing blocks can be put at the given coordinates.
///
/// This includes checking:
/// * if the coordinates are already filled with objects
/// * if the coordinates must be kept open
/// * if blocking the coordinates splits the walkable area in two distinct regions
fn is_blocking_safe(
    interior_placement_state_map: &InteriorPlacementStateMap,
    blocking_coordinates: &[(usize, usize, usize)]
) -> bool {
    // Not safe if any coordinates must be kept open, or are already occupied
    for coordinates in blocking_coordinates {
        if let Some(InteriorPlacementState::KeepOpen(_))
        | Some(InteriorPlacementState::OccupiedBlocking)
        | Some(InteriorPlacementState::OccupiedOpen)
        = interior_placement_state_map.get(coordinates) {
            return false;
        }
    }

    // Get map of walkable areas
    let open_floor_map: HashSet<(usize, usize)> = interior_placement_state_map.iter()
        .filter_map(|(coordinates, state)| {
            if coordinates.1 == 0 && state.is_open() {
                Some((coordinates.0, coordinates.2))
            } else {
                None
            }
        })
        .collect();
    let open_head_height_map: HashSet<(usize, usize)> = interior_placement_state_map.iter()
        .filter_map(|(coordinates, state)| {
            if coordinates.1 == 1 && state.is_open() {
                Some((coordinates.0, coordinates.2))
            } else {
                None
            }
        })
        .collect();
    let walkable_map: HashSet<(usize, usize)> =  open_floor_map.intersection(&open_head_height_map).copied().collect();

    // Find block (x, z) coordinates that if placed will block movement
    let movement_blocking_coordinates: HashSet<(usize, usize)> = blocking_coordinates.iter()
        .filter(|coordinates| coordinates.1 < 2) // Must be in one of bottom two layers
        .map(|coordinates| (coordinates.0, coordinates.2)) // Only x and z coordinates
        .collect();

    // Remove the blocking coordinates from the walkable map
    let walkable_map: HashSet<(usize, usize)> = walkable_map.difference(&movement_blocking_coordinates).copied().collect();

    // Find neighbour coordinates of the blocking coordinates
    let mut neighbours: HashSet<(usize, usize)> = HashSet::new();
    for blocking in &movement_blocking_coordinates {
        for neighbour in neighbourhood_4(*blocking) {
            neighbours.insert(neighbour);
        }
    }
    // Don't include the blocking coordinates themselves
    neighbours = neighbours.difference(&movement_blocking_coordinates).copied().collect();
    // Only include neighbours that are in walkable_map
    neighbours = neighbours.intersection(&walkable_map).copied().collect();

    if neighbours.len() <= 1 {
        // With 0 or 1 walkable neighbours, it is impossible for the blocking tiles to block
        // walkability. It is therefore safe to block the gien set of coordinates.
        return true;
    }

    is_subset_connected(&walkable_map, &neighbours)
}

/// Checks if walk-through blocks can be put at the given coordinates.
///
/// This includes checking:
/// * if the coordinates are already filled with objects
fn is_nonblocking_safe(
    interior_placement_state_map: &InteriorPlacementStateMap,
    blocking_coordinates: &[(usize, usize, usize)]
) -> bool {
    // Not safe if any coordinates are already occupied
    for coordinates in blocking_coordinates {
        if let Some(InteriorPlacementState::OccupiedBlocking)
        | Some(InteriorPlacementState::OccupiedOpen)
        = interior_placement_state_map.get(coordinates) {
            return false;
        }
    }

    // If not proven otherwise, putting down the blocks is safe.
    true
}

fn is_open(state_map: &InteriorPlacementStateMap, coordinates: (usize, usize, usize)) -> bool {
    if let Some(state) = state_map.get(&coordinates) {
        state.is_open()
    } else {
        false
    }
}

fn neighbourhood_4((x, z): (usize, usize)) -> Vec<(usize, usize)> {
    let mut neighbourhood_coordinates = vec![(x + 1, z), (x, z + 1)];
    if x > 0 { neighbourhood_coordinates.push((x - 1, z)) }
    if z > 0 { neighbourhood_coordinates.push((x, z - 1)) }
    neighbourhood_coordinates
}

fn neighbourhood_4_3d((x, y, z): (usize, usize, usize)) -> Vec<(usize, usize, usize)> {
    let mut neighbourhood_coordinates = vec![(x + 1, y, z), (x, y, z + 1)];
    if x > 0 { neighbourhood_coordinates.push((x - 1, y, z)) }
    if z > 0 { neighbourhood_coordinates.push((x, y, z - 1)) }
    neighbourhood_coordinates
}

fn neighbour_direction(current: (usize, usize), neighbour: (usize, usize)) -> Surface4 {
    if neighbour.0 > current.0 {
        Surface4::East
    } else if neighbour.0 < current.0 {
        Surface4::West
    } else if neighbour.1 > current.1 {
        Surface4::South
    } else if neighbour.1 < current.1 {
        Surface4::North
    } else {
        warn!("Trying to get direction to same coordinates: {:?}", current);
        Surface4::North
    }
}

fn neighbour_in_direction(current: (usize, usize), direction: Surface4) -> Option<(usize, usize)> {
    let (x, z) = current;
    match direction {
        Surface4::West => if x > 0 { Some((x - 1, z)) } else { None },
        Surface4::North => if z > 0 { Some((x, z - 1)) } else { None },
        Surface4::East => Some((x + 1, z)),
        Surface4::South => Some((x, z + 1)),
    }
}

fn neighbour_in_direction_3d(current: (usize, usize, usize), direction: Surface4) -> Option<(usize, usize, usize)> {
    let (x, y, z) = current;
    match direction {
        Surface4::West => if x > 0 { Some((x - 1, y, z)) } else { None },
        Surface4::North => if z > 0 { Some((x, y, z - 1)) } else { None },
        Surface4::East => Some((x + 1, y, z)),
        Surface4::South => Some((x, y, z + 1)),
    }
}

/// Checks if all coordinates in the subset are connected via the coordinates in set.
fn is_subset_connected(set: &HashSet<(usize, usize)>, subset: &HashSet<(usize, usize)>) -> bool {
    if subset.len() < 2 {
        return true;
    }

    let source = subset.into_iter().next().expect("We know that subset has len() >= 2 from previous check.");
    let mut subset = subset.clone();
    let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
    let mut visited: HashSet<(usize, usize)> = HashSet::new();

    subset.remove(source);
    queue.push_back(*source);

    while let Some(coordinates) = queue.pop_front() {
        if visited.contains(&coordinates) {
            continue;
        }
        visited.insert(coordinates);

        let neighbours = neighbourhood_4(coordinates);
        for neighbour in neighbours {
            if !set.contains(&neighbour) {
                continue;
            }

            subset.remove(&neighbour);
            queue.push_back(neighbour);

            if subset.is_empty() {
                return true;
            }
        }
    }

    false
}

fn available_on_floor_backed(state_map: &InteriorPlacementStateMap) -> HashSet<(usize, usize, usize)> {
    state_map.iter()
        .filter_map(|(coordinates, state)| {
            if let InteriorPlacementState::Available(placement_collection) = state {
                for placement_option in placement_collection {
                    if let PlacementOption::OnFloorBacked(_) = placement_option {
                        return Some(*coordinates);
                    }
                }
                None
            } else {
                None
            }
        })
        .collect()
}

fn available_on_floor_freestanding(state_map: &InteriorPlacementStateMap) -> HashSet<(usize, usize, usize)> {
    state_map.iter()
        .filter_map(|(coordinates, state)| {
            if let InteriorPlacementState::Available(placement_collection) = state {
                for placement_option in placement_collection {
                    if let PlacementOption::OnFloorFreestanding = placement_option {
                        return Some(*coordinates);
                    }
                }
                None
            } else {
                None
            }
        })
        .collect()
}

fn available_on_floor(state_map: &InteriorPlacementStateMap) -> HashSet<(usize, usize, usize)> {
    available_on_floor_backed(state_map).union(&available_on_floor_freestanding(state_map)).copied().collect()
}

/// Returns set of coordinates on layers 0 and 1, where the coordinate for both layers are open.
fn walkable(state_map: &InteriorPlacementStateMap) -> HashSet<(usize, usize, usize)> {
    let open_floor_map: HashSet<(usize, usize)> = state_map.iter()
        .filter_map(|(coordinates, state)| {
            if coordinates.1 == 0 && state.is_open() {
                Some((coordinates.0, coordinates.2))
            } else {
                None
            }
        })
        .collect();
    let open_head_height_map: HashSet<(usize, usize)> = state_map.iter()
        .filter_map(|(coordinates, state)| {
            if coordinates.1 == 1 && state.is_open() {
                Some((coordinates.0, coordinates.2))
            } else {
                None
            }
        })
        .collect();
    let walkable_map: HashSet<(usize, usize)> =  open_floor_map.intersection(&open_head_height_map).copied().collect();

    state_map.iter()
        .filter_map(|(coordinates, _)| {
            if walkable_map.contains(&(coordinates.0, coordinates.2)) {
                Some(*coordinates)
            } else {
                None
            }
        })
        .collect()
}

fn on_floor_backed_directions(state_map: &InteriorPlacementStateMap, coordinates: (usize, usize, usize)) -> Vec<Surface4> {
    if let Some(state) = state_map.get(&coordinates) {
        if let InteriorPlacementState::Available(collection) | InteriorPlacementState::KeepOpen(collection) = state {
            return collection.iter()
                .filter_map(|option| {
                    if let PlacementOption::OnFloorBacked(direction) = option {
                        Some(*direction)
                    } else {
                        None
                    }
                })
                .collect();
        }
    }

    Vec::new()
}

fn on_wall_directions(state_map: &InteriorPlacementStateMap, coordinates: (usize, usize, usize)) -> Vec<Surface4> {
    if let Some(state) = state_map.get(&coordinates) {
        if let InteriorPlacementState::Available(collection) | InteriorPlacementState::KeepOpen(collection) = state {
            return collection.iter()
                .filter_map(|option| {
                    if let PlacementOption::OnWall(direction) = option {
                        Some(*direction)
                    } else {
                        None
                    }
                })
                .collect();
        }
    }

    Vec::new()
}

fn from_ceiling_backed_directions(state_map: &InteriorPlacementStateMap, coordinates: (usize, usize, usize)) -> Vec<Surface4> {
    if let Some(state) = state_map.get(&coordinates) {
        if let InteriorPlacementState::Available(collection) | InteriorPlacementState::KeepOpen(collection) = state {
            return collection.iter()
                .filter_map(|option| {
                    if let PlacementOption::FromCeilingBacked(direction) = option {
                        Some(*direction)
                    } else {
                        None
                    }
                })
                .collect();
        }
    }

    Vec::new()
}

fn on_top_surface_backed_directions(state_map: &InteriorPlacementStateMap, coordinates: (usize, usize, usize)) -> Vec<Surface4> {
    if let Some(state) = state_map.get(&coordinates) {
        if let InteriorPlacementState::Available(collection) | InteriorPlacementState::KeepOpen(collection) = state {
            return collection.iter()
                .filter_map(|option| {
                    if let PlacementOption::OnTopSurfaceBacked(direction) = option {
                        Some(*direction)
                    } else {
                        None
                    }
                })
                .collect();
        }
    }

    Vec::new()
}

fn on_side_surface_directions(state_map: &InteriorPlacementStateMap, coordinates: (usize, usize, usize)) -> Vec<Surface4> {
    if let Some(state) = state_map.get(&coordinates) {
        if let InteriorPlacementState::Available(collection) | InteriorPlacementState::KeepOpen(collection) = state {
            return collection.iter()
                .filter_map(|option| {
                    if let PlacementOption::OnSideSurface(direction) = option {
                        Some(*direction)
                    } else {
                        None
                    }
                })
                .collect();
        }
    }

    Vec::new()
}

fn any_directions(state_map: &InteriorPlacementStateMap, coordinates: (usize, usize, usize)) -> Vec<Surface4> {
    if let Some(state) = state_map.get(&coordinates) {
        if let InteriorPlacementState::Available(collection) | InteriorPlacementState::KeepOpen(collection) = state {
            return collection.iter()
                .filter_map(|option| {
                    if let PlacementOption::OnFloorBacked(direction)
                    | PlacementOption::OnWall(direction)
                    | PlacementOption::FromCeilingBacked(direction)
                    | PlacementOption::OnTopSurfaceBacked(direction)
                    | PlacementOption::OnSideSurface(direction) = option {
                        Some(*direction)
                    } else {
                        None
                    }
                })
                .collect();
        }
    }

    Vec::new()
}

// Functions for placing objects / fulfilling room requirement
///////////////////////////////////////////////////////////////

/// Place objects fulfilling the "cooking" requirement, e.g. a furnace, or smoker.
fn place_cooking(excerpt: &mut WorldExcerpt, state_map: &mut InteriorPlacementStateMap) -> bool {
    let walkable_tiles = walkable(&state_map);

    for location in available_on_floor_backed(&state_map) {
        for direction in on_floor_backed_directions(state_map, location) {
            let direction = direction.opposite();
            if let Some(neighbour) = neighbour_in_direction_3d(location, direction) {
                if walkable_tiles.contains(&neighbour)
                && is_blocking_safe(&state_map, &[location]) {
                    excerpt.set_block_at(
                        BlockCoord(location.0 as i64, location.1 as i64, location.2 as i64),
                        Block::furnace(direction),
                    );

                    // Mark the location of the furnace and the volume in front of it
                    state_map_mark_blocking(state_map, location);
                    state_map_mark_open(state_map, neighbour);

                    // Let other objects connect to the sides of the furnace
                    if let Some(neighbour) = neighbour_in_direction_3d(location, direction.rotated_90_ccw()) {
                        state_map_add_backing(state_map, neighbour, direction.rotated_90_cw());
                    }
                    if let Some(neighbour) = neighbour_in_direction_3d(location, direction.rotated_90_cw()) {
                        state_map_add_backing(state_map, neighbour, direction.rotated_90_ccw());
                    }

                    // Let other objects be placed on top of the furnace
                    state_map_add_top_surface(state_map, (location.0, location.1 + 1, location.2));

                    return true;
                }
            }
        }
    }

    false
}

/// Place objects fulfilling the "hygiene" requirement, e.g. some washing utility.
fn place_hygiene(excerpt: &mut WorldExcerpt, state_map: &mut InteriorPlacementStateMap) -> bool {
    let walkable_tiles = walkable(&state_map);

    let candidates: Vec<(usize, usize, usize)> = available_on_floor_backed(&state_map)
        .into_iter()
        .chain(
            available_on_floor_freestanding(&state_map)
            .into_iter()
        )
        .collect();

    for location in candidates {
        for neighbour in neighbourhood_4_3d(location) {
            if walkable_tiles.contains(&neighbour)
            && is_blocking_safe(&state_map, &[location]) {
                let mut rng = thread_rng();
                let water_level = mcprogedit::bounded_ints::Int0Through3::new(rng.gen_range(0..=3)).unwrap();

                excerpt.set_block_at(
                    BlockCoord(location.0 as i64, location.1 as i64, location.2 as i64),
                    Block::Cauldron { water_level },
                );
                state_map_mark_blocking(state_map, location);
                state_map_mark_open(state_map, neighbour);
                return true;
            }
        }
    }

    false
}

/// Place light sources. Returns true if enough light sources was placed that the area is
/// completely illuminated.
fn place_lighting(excerpt: &mut WorldExcerpt, state_map: &mut InteriorPlacementStateMap) -> bool {
    const LANTERN_BRIGHTNESS: usize = 15;
    const TORCH_BRIGHTNESS: usize = 14;

    // Internal function for getting light coordinates to remove
    fn illuminated_coordinates(light_position: (usize, usize, usize), intensity: usize) -> Vec<(usize, usize)> {
        const LIGHT_LEVEL_MIN: usize = 8;
        let (light_x, light_y, light_z) = light_position;
        let radius = intensity - light_y - LIGHT_LEVEL_MIN;

        let mut output = Vec::new();

        for x in light_x.saturating_sub(radius) .. light_x + radius + 1 {
            for z in light_z.saturating_sub(radius) .. light_z + radius + 1 {
                let distance_from_light = max(light_x, x) - min(light_x, x) + max(light_z, z) - min(light_z, z);
                if distance_from_light <= radius {
                    output.push((x, z));
                }
            }
        }

        output
    }

    // These are the positions that should get illuminated
    let mut darkness_map: HashSet<(usize, usize)> = state_map.iter()
        .map(|((x, _, z), _)| (*x, *z))
        .collect();

    // Potential lantern locations: Top surfaces.
    let top_surface_positions: InteriorPlacementStateMap = state_map.iter()
        .filter_map(|((x, y, z), state)| {
            if let InteriorPlacementState::Available(collection)
            | InteriorPlacementState::KeepOpen(collection) = state {
                for option in collection {
                    match option {
                        PlacementOption::OnTopSurfaceFreestanding
                        | PlacementOption::OnTopSurfaceBacked(_) => {
                            if *y == 1 || *y == 2 {
                                return Some(((*x, *y, *z), state.clone()));
                            }
                        }
                        _ => (),
                    }
                }
                None
            } else {
                None
            }
        })
        .collect();

    // Potential lantern locations: Hanging from ceiling.
    let ceiling_positions: InteriorPlacementStateMap = state_map.iter()
        .filter_map(|((x, y, z), state)| {
            if let InteriorPlacementState::Available(collection)
            | InteriorPlacementState::KeepOpen(collection) = state {
                for option in collection {
                    match option {
                        PlacementOption::FromCeilingFreestanding
                        | PlacementOption::FromCeilingBacked(_) => {
                            if *y >= 2 {
                                return Some(((*x, *y, *z), state.clone()));
                            }
                        }
                        _ => (),
                    }
                }
                None
            } else {
                None
            }
        })
        .collect();

    // Potential torch locations: On walls.
    let torch_positions: InteriorPlacementStateMap = state_map.iter()
        .filter_map(|((x, y, z), state)| {
            if let InteriorPlacementState::Available(collection)
            | InteriorPlacementState::KeepOpen(collection) = state {
                for option in collection {
                    match option {
                        PlacementOption::OnWall(_)
                        | PlacementOption::OnSideSurface(_) => {
                            if *y >= 1 {
                                return Some(((*x, *y, *z), state.clone()));
                            }
                        }
                        _ => (),
                    }
                }
                None
            } else {
                None
            }
        })
        .collect();

    // Potential torch positions: On floor.
    let floor_positions: InteriorPlacementStateMap = state_map.iter()
        .filter_map(|((x, y, z), state)| {
            if let InteriorPlacementState::Available(collection)
            | InteriorPlacementState::KeepOpen(collection) = state {
                for option in collection {
                    match option {
                        PlacementOption::OnFloorFreestanding
                        | PlacementOption::OnFloorBacked(_) => {
                            return Some(((*x, *y, *z), state.clone()));
                        }
                        _ => (),
                    }
                }
                None
            } else {
                None
            }
        })
        .collect();

    // Put lanterns on surfaces
    for ((x, y, z), _) in top_surface_positions {
        if darkness_map.contains(&(x, z))
        && is_nonblocking_safe(&state_map, &[(x, y, z)]) {
            // Place lantern
            excerpt.set_block_at(
                BlockCoord(x as i64, y as i64, z as i64),
                Block::Lantern { mounted_at: Surface2::Down, waterlogged: false },
            );
            // Bookkeeping
            state_map_mark_occupied_open(state_map, (x, y, z));
            // Remove surroundings from darkness map
            for surroundings in illuminated_coordinates((x, y, z), LANTERN_BRIGHTNESS) {
                darkness_map.remove(&surroundings);
            }
        }
    }

    // TODO Lantern from ceiling

    // Put torches on walls
    for ((x, y, z), state) in torch_positions {
        if darkness_map.contains(&(x, z))
        && is_nonblocking_safe(&state_map, &[(x, y, z)]) {
            // Get torch attachment surface
            let direction: Direction = on_wall_directions(state_map, (x, y, z))
                .pop()
                .expect("Torch positions are on wall, so we should get at least one direction match.")
                .into();
            let direction: Surface5 = direction
                .try_into()
                .expect("Going from Surface4 to Surface5 should be safe.");

            // Place torch
            excerpt.set_block_at(
                BlockCoord(x as i64, y as i64, z as i64),
                Block::Torch { attached: direction },
            );
            // Bookkeeping
            state_map_mark_occupied_open(state_map, (x, y, z));
            // Remove surroundings from darkness map
            for surroundings in illuminated_coordinates((x, y, z), TORCH_BRIGHTNESS) {
                darkness_map.remove(&surroundings);
            }
        }
    }

    // Last fallback: Put torch on floor
    for ((x, y, z), state) in floor_positions {
        if darkness_map.contains(&(x, z))
        && is_nonblocking_safe(&state_map, &[(x, y, z)]) {
            // Place torch
            excerpt.set_block_at(
                BlockCoord(x as i64, y as i64, z as i64),
                Block::Torch { attached: Surface5::Down },
            );
            // Bookkeeping
            state_map_mark_occupied_open(state_map, (x, y, z));
            // Remove surroundings from darkness map
            for surroundings in illuminated_coordinates((x, y, z), TORCH_BRIGHTNESS) {
                darkness_map.remove(&surroundings);
            }
        }
    }

    // TODO What to do if not completely lighted???

    if darkness_map.is_empty() {
        true
    } else {
        false
    }
}

// TODO place_double_sleep

/// Place objects fulfilling the "sleep" requirement for one person, e.g. a bed.
fn place_single_sleep(excerpt: &mut WorldExcerpt, state_map: &mut InteriorPlacementStateMap) -> bool {
    // Find all ground tiles with wall (or other) backing, for bed head end.
    let on_floor_backed_tiles = available_on_floor_backed(&state_map);
    let on_floor_tiles = available_on_floor(&state_map);
    let walkable_tiles = walkable(&state_map);

    // TODO Iterate sorted by distance from door (farther away is better)
    // TODO Prefer walkable tiles already marked for keeping open
    // TODO Prefer walkable tiles to the side of the bed over walkable tiles behind it
    for candidate_head_end in on_floor_backed_tiles {
        // Find adjacent tiles which may be used for foot end of bed
        for candidate_foot_end in neighbourhood_4((candidate_head_end.0, candidate_head_end.2))
                .iter()
                .map(|(x, z)| (*x, candidate_head_end.1, *z))
                .filter(|c| on_floor_tiles.contains(&c)) {
            for candidate_open_tile in neighbourhood_4((candidate_foot_end.0, candidate_foot_end.2))
                    .iter()
                    .map(|(x, z)| (*x, candidate_foot_end.1, *z))
                    .filter(|c| walkable_tiles.contains(&c) && *c != candidate_head_end) {
                if is_blocking_safe(&state_map, &[candidate_head_end, candidate_foot_end]) {
                    let he = candidate_head_end;
                    let fe = candidate_foot_end;

                    let mut rng = thread_rng();
                    let colour: Colour = rng.gen_range(0..=15).into();

                   // let colour = Colour::Red;
                    let facing = neighbour_direction((fe.0, fe.2), (he.0, he.2));
                    let head_end = BlockCoord(he.0 as i64, he.1 as i64, he.2 as i64);
                    let foot_end = BlockCoord(fe.0 as i64, fe.1 as i64, fe.2 as i64);

                    excerpt.set_block_at(
                        head_end,
                        Block::Bed(
                            mcprogedit::block::Bed { colour, facing, end: mcprogedit::block::BedEnd::Head }
                        )
                    );
                    excerpt.set_block_at(
                        foot_end,
                        Block::Bed(
                            mcprogedit::block::Bed { colour, facing, end: mcprogedit::block::BedEnd::Foot }
                        )
                    );

                    // State bookkeeping
                    state_map_mark_blocking(state_map, candidate_head_end);
                    state_map_mark_blocking(state_map, candidate_foot_end);
                    state_map_mark_open(state_map, candidate_open_tile);

                    return true;
                }
            }
        }
    }

    false
}

/// Place objects fulfilling the "store" requirement, e.g. a chest, or barrel.
fn place_store(excerpt: &mut WorldExcerpt, state_map: &mut InteriorPlacementStateMap) -> bool {
    let walkable_tiles = walkable(&state_map);

    for location in available_on_floor_backed(&state_map) {
        let above: (usize, usize, usize) = (location.0, location.1 + 1, location.2);

        if !is_open(&state_map, above) {
            continue;
        }

        for direction in on_floor_backed_directions(state_map, location) {
            let direction = direction.opposite();
            if let Some(neighbour) = neighbour_in_direction_3d(location, direction) {
                if walkable_tiles.contains(&neighbour)
                && is_blocking_safe(&state_map, &[location]) {
                    excerpt.set_block_at(
                        BlockCoord(location.0 as i64, location.1 as i64, location.2 as i64),
                        Block::chest(direction),
                    );
                    state_map_mark_blocking(state_map, location);
                    state_map_mark_open(state_map, neighbour);
                    state_map_mark_open(state_map, above);
                    return true;
                }
            }
        }
    }

    false
}

// Utility functions for operating on InteriorPlacementStateMap
////////////////////////////////////////////////////////////////

fn state_map_mark_occupied_open(state_map: &mut InteriorPlacementStateMap, coordinates: (usize, usize, usize)) {
    state_map.insert(coordinates, InteriorPlacementState::OccupiedOpen);
    // TODO Check first if already an Occupied state, and if so return an error.
}

fn state_map_mark_open(state_map: &mut InteriorPlacementStateMap, coordinates: (usize, usize, usize)) {
    let current = state_map.entry(coordinates).or_insert(InteriorPlacementState::KeepOpen(HashSet::new()));
    if let InteriorPlacementState::Available(collection) = current {
        *current = InteriorPlacementState::KeepOpen(collection.clone());
    }
    // TODO Return an error if *current == OccupiedBlocking, as then it is already blocking and
    // cannot be kept open.
}

fn state_map_mark_blocking(state_map: &mut InteriorPlacementStateMap, coordinates: (usize, usize, usize)) {
    state_map.insert(coordinates, InteriorPlacementState::OccupiedBlocking);
    // TODO Check first if already an Occupied state, and if so return an error.
}

fn state_map_add_backing(
    state_map: &mut InteriorPlacementStateMap,
    coordinates: (usize, usize, usize),
    direction: Surface4,
) {
    // TODO Look up the collection
    // TODO Figure out what kind of placement (floor vs wall vs ceiling vs surface)
    // TODO Add backing
    // TODO Remove any "freestanding"
}

fn state_map_add_top_surface(
    state_map: &mut InteriorPlacementStateMap,
    coordinates: (usize, usize, usize),
) {
    let directions = any_directions(state_map, coordinates);

    match state_map.get_mut(&coordinates) {
        Some(InteriorPlacementState::Available(collection))
        | Some(InteriorPlacementState::KeepOpen(collection)) => {
            if directions.is_empty() {
                collection.insert(PlacementOption::OnTopSurfaceFreestanding);
            } else {
                for direction in directions {
                    collection.insert(PlacementOption::OnTopSurfaceBacked(direction));
                }
            }
        }
        _ => (),
    }
}

fn state_map_add_side_surface(
    state_map: &mut InteriorPlacementStateMap,
    coordinates: (usize, usize, usize),
    direction: Surface4,
) {
    match state_map.get_mut(&coordinates) {
        Some(InteriorPlacementState::Available(collection))
        | Some(InteriorPlacementState::KeepOpen(collection)) => {
            collection.insert(PlacementOption::OnSideSurface(direction));
        }
        _ => (),
    }
}

/*
type InteriorPlacementStateMap = HashMap<(usize, usize, usize), InteriorPlacementState>;
enum InteriorPlacementState {
    Available(PlacementOptionCollection), // Position is available for any object placement.
    KeepOpen(PlacementOptionCollection), // Position is available for non-blocking objects only.
    OccupiedBlocking, // There's an object there which blocks movement.
    OccupiedOpen, // There's an object there which does not block movement.
}
type PlacementOptionCollection = HashSet<PlacementOption>;
enum PlacementOption {
    OnWall(Surface4), // Registered surface is facing the wall
    OnFloorFreestanding,
    OnFloorBacked(Surface4), // Registered surface is facing wall or object
    FromCeilingFreestanding,
    FromCeilingBacked(Surface4), // Registered surface is facing wall or object
    OnTopSurfaceBacked(Surface4), // Registered surface is facing wall or object
    OnTopSurfaceFreestanding,
    OnSideSurface(Surface4), // Registered surface is facing the neighbouring object
}
*/

// Functions for placing objects:
// Takes (&WorldExcerpt, &InteriorPlacementStateMap, budget),
// places object(s) within budget number of blocks placed,
// returns whether or not it succeeded (bool, Result<(), ()> or some enum).
// TODO Function for placing "store"
// TODO Function for placing "eat"
// TODO Function for placing "light"
// TODO Function for placing "decor"
// TODO Function for placing "sit"
// TODO Function for placing "study"

// Functions for furnishing rooms:
// Takes (&RoomShape), returns WorldExcerpt containing the furniture.
// TODO Function for furnishing "cottage":
//      - Requires: "sleep", "cook", "store", "light"
//      - Wants: "eat", "decor"
// TODO Function for furnishing "bedroom":
//      - Requires: "sleep", "light"
//      - Wants: "store", "decor", "study", "sit"
// TODO Debug function: Marks all "Available" "Floor" locations with glass block.


// Functions for furnishing various rooms
//////////////////////////////////////////

pub fn furnish_debug(room_shape: &RoomShape) -> Option<WorldExcerpt> {
    let mut placement_state_map = interior_placement_state_map_from_room_shape(&room_shape);

    // Create a world excerpt, for placing objects into
    let (x, z) = room_shape.dimensions();
    let mut output = WorldExcerpt::new(x, 2, z);

    // TODO Fill that output with carpets/hooks showing initial state of where things can be placed.
    // TODO put buttons or something on wall positions, some transparent floating block in mid-air
    for ((x, y, z), placement_state) in placement_state_map.iter() {
        match placement_state {
            InteriorPlacementState::Available(state_collection) => {
                output.set_block_at(
                    BlockCoord(*x as i64, *y as i64, *z as i64),
                    Block::carpet_with_colour(Colour::Yellow),
                );
            }
            InteriorPlacementState::KeepOpen(state_collection) => {
                output.set_block_at(
                    BlockCoord(*x as i64, *y as i64, *z as i64),
                    Block::carpet_with_colour(Colour::Red),
                );
            }
            _ => (),
        }
    }

    Some(output)
}

pub fn furnish_cottage(room_shape: &RoomShape) -> Option<WorldExcerpt> {
    let mut placement_state_map = interior_placement_state_map_from_room_shape(&room_shape);

    let (x, z) = room_shape.dimensions();
    let mut output = WorldExcerpt::new(x, 2, z);

    place_single_sleep(&mut output, &mut placement_state_map);
    place_cooking(&mut output, &mut placement_state_map);
    place_store(&mut output, &mut placement_state_map);
    place_hygiene(&mut output, &mut placement_state_map);
    // TODO Place also "top_surface", "decor"

    place_lighting(&mut output, &mut placement_state_map);

    place_single_sleep(&mut output, &mut placement_state_map);

    // TODO Put down a reasonable number of beds, not as many as the algorithm is able to fit!
    while place_single_sleep(&mut output, &mut placement_state_map) {}


    Some(output)
}

