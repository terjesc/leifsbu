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

    // TODO Generate a RoomShape from coordinate lists for walls, doors, windows.
    // pub fn from_walls_dors_and_windows() -> Self {
    // }

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

type InteriorPlacementStateMap = HashMap<(usize, usize, usize), InteriorPlacementState>;

fn interior_placement_state_map_from_room_shape(room_shape: &RoomShape) -> InteriorPlacementStateMap {
    let mut output = HashMap::new();

    let (x_len, z_len) = room_shape.dimensions();
    let height = 2; // TODO get actual ceiling heights from the room_shape instead, in below foor loops

    for x in 0..x_len {
        for z in 0..z_len {
            if let Some(ColumnKind::Floor) = room_shape.column_at((x, z)) {
                for height in 0..height {
                    let mut available_placements = PlacementOptionCollection::new();
                    let mut neighbourhood_coordinates = vec![(x + 1, z), (x, z + 1)];
                    if x > 0 { neighbourhood_coordinates.push((x - 1, z)) }
                    if z > 0 { neighbourhood_coordinates.push((x, z - 1)) }

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

// NB What directions must be unoccupied, around an already placed object?
//      * Use InteriorPlacementState::KeepOpen and ::OccupiedOpen to mark places that need to be open.
//      * For corner objects: Just live with the fact that both or a random direction is KeepOpen.


// TODO Put InteriorPlacementState in some structure (map from coordinates to state?), to:
//      1) Keep track of where it is still possible to put objects.
//      2) Keep track of positions that must be open for movement and/or object access.
//      3) Allow for "walkability" checks, keeping the walkable area contiguous.


// NB TODO FIXME This whole (unfinished) InteriorVoxelState and InteriorVoxelStateCuboid thing can be nuked.
/*
#[derive(Clone, Copy, Debug)]
enum InteriorVoxelState {
    OutOfBounds, // Outside the editable area
    Available, // Objects can be placed in this voxel
    Occupied, // Object already placed in this voxel
    KeepOpen, // Voxel must be kept available for passage
    OpenWithObject, // Voxel is available for passage, but has a passable object.
}

// TODO InteriorVoxelStateSpace: Structure for placed objects (3D structure of InteriorVoxelState)
/// 3D voxel map of the furnishing state of the room.
#[derive(Clone, Debug)]
struct InteriorVoxelStateCuboid {
    voxels: Vec<InteriorVoxelState>,
    x_dim: usize,
    y_dim: usize,
    z_dim: usize,
}

impl InteriorVoxelStateCuboid {

    /// Returns a new InteriorVoxelStateSpace of the given dimensions, with all voxels marked out-of-bounds.
    pub fn new((x_dim, y_dim, z_dim): (usize, usize, usize)) -> Self {
        Self::new_filled((x_dim, y_dim, z_dim), InteriorVoxelState::OutOfBounds)
    }

    /// Returns a new InteriorVoxelStateSpace of the given dimensions, with all voxels set to `voxel_state`.
    pub fn new_filled(
        (x_dim, y_dim, z_dim): (usize, usize, usize),
        voxel_state: InteriorVoxelState,
    ) -> Self {
        let voxels_len = x_dim * y_dim * z_dim;
        let voxels = vec![voxel_state; voxels_len];

        Self { voxels, x_dim, y_dim, z_dim }
    }

    /// Generates an InteriorVoxelStateCuboid from a RoomState.
    pub fn from_room_shape(room_shape: &RoomShape) -> Self {
        let (x_len, z_len) = room_shape.dimensions();
        let mut cuboid = Self::new((x_len, 2, z_len));

        // We must make sure every open square has access to a chosen open sink square.
        //      Some(direction): Access to sink via the square to that direction.
        //      None => No access, or is wall or window or out of bounds or whatever.
        let mut door_access: HashMap<(usize, usize), Option<Surface4>> = HashMap::new();

        // Initialize with None (meaning no route to sink.)
        for x in 0..x_len {
            for z in 0..z_len {
                match room_shape.column_at((x, z)) {
                    Some(ColumnKind::Floor)
                    | Some(ColumnKind::Door) => { door_access.insert((x, z), None); }
                    None => unreachable!(),
                    _ => (),
                }
            }
        }

        // If there are no doors or floors, then everything is out-of-bounds.
        if door_access.is_empty() {
            return cuboid;
        }

        // Pick a door as sink square. This ensures navigable path to the door, from any point
        // within the room, and from any other door bordering the room.
        // TODO pick a door as sink, instead of random node, and if no door pick a floor.
        let sink: (usize, usize) = door_access.keys().copied().next().unwrap();

        // Flood fill from the chosen sink
        let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
        queue.push_back(sink);
        while let Some((x, z)) = queue.pop_front() {
            // For each neigbour,
            // if neighbour is None (i.e. undecided as of yet),
            // add direction and push neighbour to queue
            if x > 0 {
                let neighbour_coordinates = (x - 1, z);
                if let Some(direction) = door_access.get_mut(&neighbour_coordinates) {
                    if *direction == None {
                        queue.push_back(neighbour_coordinates);
                        *direction = Some(Surface4::East);
                    }
                }
            }
            if x < x_len - 1 {
                let neighbour_coordinates = (x + 1, z);
                if let Some(direction) = door_access.get_mut(&neighbour_coordinates) {
                    if *direction == None {
                        queue.push_back(neighbour_coordinates);
                        *direction = Some(Surface4::West);
                    }
                }
            }
            if z > 0 {
                let neighbour_coordinates = (x, z - 1);
                if let Some(direction) = door_access.get_mut(&neighbour_coordinates) {
                    if *direction == None {
                        queue.push_back(neighbour_coordinates);
                        *direction = Some(Surface4::North);
                    }
                }
            }
            if z < z_len - 1 {
                let neighbour_coordinates = (x, z + 1);
                if let Some(direction) = door_access.get_mut(&neighbour_coordinates) {
                    if *direction == None {
                        queue.push_back(neighbour_coordinates);
                        *direction = Some(Surface4::South);
                    }
                }
            }
        }

        // NB 'door_access' now keeps track of how every location is reachable from a traversable
        // square connected to the entrance of the room. Whenever a traversable square is changed
        // to block traversing, first it is checked if door access for neighbouring squares can be
        // redirected to other traversable squares with paths to the entrance. If so, the square
        // can be changed to block traversing. If, after redirection attempt for all neighbours,
        // one or more neighbours still rely on the square for traversing to the entrance, then the
        // square cannot be changed from traversable to untraversable.

        // TODO use room shape properties to fill cuboid with Available and KeepOpen as seen fit.
        // For first attempt, try to put things along walls only.
        for x in 0..x_len {
            for z in 0..z_len {
                if let Some(ColumnKind::Floor) = room_shape.column_at((x, z)) {
                    if (x, z) == sink {
                        cuboid.set_voxel_state_at((x, 0, z), InteriorVoxelState::KeepOpen);
                        cuboid.set_voxel_state_at((x, 1, z), InteriorVoxelState::KeepOpen);
                        continue;
                    }
                    // TODO Check neighbours:
                    // if blocks door access: KeepOpen
                    // else if window neighbour exist: bottom block is Available, top is KeepOpen
                    // else if wall neighbour exist: both blocks are Available
                    // else: KeepOpen
                }
            }
        }

        //
        // TODO When about to designate an "Available", consider all neighbours.
        //          If they reach sink through other nodes than this one, everything fine.
        //          If they reach sink through this node:
        //              If they can be redirected to sink through a different node, switch,
        //              everything is fine.
        //              If they cannot be redirected, this node is KeepOpen.
        //          Whenever "everything fine", this node can be set "Available"

        // TODO Use room shape properties to fill cuboid with Available and KeepOpen as seen fit.

        cuboid
    }

    /// Get the dimensions of this RoomShape, as `(x_dimension, z_dimension)`.
    pub fn dimensions(&self) -> (usize, usize, usize) {
        (self.x_dim, self.y_dim, self.z_dim)
    }

    /// Set the voxel state at the (x, y, z) location `coordinates` to the given voxel state.
    pub fn set_voxel_state_at(
        &mut self,
        coordinates: (usize, usize, usize),
        voxel_state: InteriorVoxelState,
    ) {
        if let Some(index) = self.index(coordinates) {
            self.voxels[index] = voxel_state;
        }
    }

    /// Get the voxel state at the (x, y, z) location `coordinates`.
    pub fn voxel_state_at(&self, coordinates: (usize, usize, usize)) -> Option<InteriorVoxelState> {
        self.index(coordinates).map(|index| *self.voxels.get(index).unwrap())
    }

    fn index(&self, (x, y, z): (usize, usize, usize)) -> Option<usize> {
        if x >= self.x_dim || y >= self.y_dim || z >= self.z_dim {
            None
        } else {
            Some(self.y_dim * self.z_dim * x + self.y_dim * z + y)
        }
    }

}
*/

// Functions for placing objects:
// Takes (&RoomShape, &InteriorLayout, &mut WorldExcerpt, budget: usize),
// places object(s) within budget number of blocks placed,
// returns whether or not it succeeded (bool, Result<(), ()> or some enum).
// TODO Function for placing "sleep"
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

pub fn furnish_debug(room_shape: &RoomShape) -> Option<WorldExcerpt> {
    // TODO FIXME replace with calculating
//    let interior_voxel_states = InteriorVoxelStateCuboid::from_room_shape(&room_shape);

    let placement_state_map = interior_placement_state_map_from_room_shape(&room_shape);

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

