use std::collections::{HashMap, HashSet, VecDeque};

use mcprogedit::block::Block;
use mcprogedit::colour::Colour;
use mcprogedit::coordinates::BlockCoord;
use mcprogedit::positioning::Surface4;
use mcprogedit::world_excerpt::WorldExcerpt;

use log::{warn};


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
    pub fn column_at(&self, coordinates: (usize, usize)) -> Option<ColumnKind> {
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
            if let Some(ColumnKind::Floor) = room_shape.column_at((x, z)) {
                for height in 0..height {
                    let mut available_placements = PlacementOptionCollection::new();
                    let neighbourhood_coordinates = neighbourhood_4((x, z));
                    let mut must_be_kept_open = false;

                    if height == 0 {
                        // Ground level
                        for neighbour_coordinates in neighbourhood_coordinates {
                            match room_shape.column_at(neighbour_coordinates) {
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
                            match room_shape.column_at(neighbour_coordinates) {
                                Some(ColumnKind::Wall) => {
                                    let direction = neighbour_direction((x, z), neighbour_coordinates);
                                    available_placements.insert(PlacementOption::OnWall(direction));
                                }
                                Some(ColumnKind::Window)
                                | Some(ColumnKind::Door) => must_be_kept_open = true,
                                _ => (),
                            }
                        }
                        if available_placements.is_empty() {
                            available_placements.insert(PlacementOption::OnFloorFreestanding);
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

fn neighbourhood_4((x, z): (usize, usize)) -> Vec<(usize, usize)> {
    let mut neighbourhood_coordinates = vec![(x + 1, z), (x, z + 1)];
    if x > 0 { neighbourhood_coordinates.push((x - 1, z)) }
    if z > 0 { neighbourhood_coordinates.push((x, z - 1)) }
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
}
*/

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

// Functions for placing objects / fulfilling room requirement
///////////////////////////////////////////////////////////////

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

                    let colour = Colour::Red;
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

// Functions for placing objects:
// Takes (&WorldExcerpt, &InteriorPlacementStateMap, budget),
// places object(s) within budget number of blocks placed,
// returns whether or not it succeeded (bool, Result<(), ()> or some enum).
// TODO Function for placing "cook"
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

    // TODO Put down a reasonable number of beds, not as many as the algorithm is able to fit!
    while place_single_sleep(&mut output, &mut placement_state_map) { ; }

    Some(output)
/*
    if place_single_sleep(&mut output, &mut placement_state_map) {
        Some(output)
    } else {
        None
    }
*/
    // TODO Place also "cook", "store" and "light"
}

