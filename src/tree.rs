use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::TryFrom;

use mcprogedit::block::{Block, Log};
use mcprogedit::coordinates::BlockCoord;
use mcprogedit::material::{LeavesMaterial, WoodMaterial};
use mcprogedit::world_excerpt::WorldExcerpt;

/// If the given location holds part of a tree, remove that part of the tree and
/// any other parts of the tree that are further "out", i.e. further away from
/// the trunk, further out on/away from the branch, etc.
///
/// If the given location holds vines, remove the vines from there down.
pub fn prune(excerpt: &mut WorldExcerpt, at: BlockCoord) {
    match excerpt.block_at(at) {
        Some(Block::Log(_)) | Some(Block::Leaves { .. }) => {
            // TODO get the database
            // TODO remove block and all descendants, and prune any connected vines
        }
        Some(Block::Vines(_)) => {
            // Remove all vines going downwards
            for y in (0..=at.1).rev() {
                if let Some(Block::Vines(_)) = excerpt.block_at((at.0, y, at.2).into()) {
                    excerpt.set_block_at((at.0, y, at.2).into(), Block::Air);
                } else {
                    break;
                }
            }
        }
        _ => (),
    }
}

/// If the given location holds part of a tree, remove the whole tree.
pub fn chop(excerpt: &mut WorldExcerpt, at: BlockCoord) {
    let to_chop = find_tree(excerpt, &at);

    if to_chop.len() > 0 {
        println!("Found a tree, need to remove {} blocks!", to_chop.len());
    }

    for coordinates in to_chop {
        excerpt.set_block_at(coordinates, Block::Air);
    }
    /*
    match excerpt.block_at(at) {
        Some(Block::Log(_)) | Some(Block::Leaves { .. }) => {
            // NB Inefficient implementation!
            //    Recalculates for whole excerpt, each and every time.
            let database = prepare(excerpt);

            if let Some(TreeBlockInfo { tree_id, .. }) = database.get(&at) {
                println!("Need to chop down tree #{}!", tree_id);
                // TODO remove all nearby blocks of that tree_id,
                //      if they are of that tree's log or leaf type.
                //      Also remove from cached databas (if any),
                //      and prune any vines that cannot hang from anything else
            }
        }
        _ => (),
    }
    */
}

struct TreeBlockInfo {
    tree_id: usize,
    parent: Option<BlockCoord>,
}

/// Prepare a database of trees
///
/// The range needed around a single block in order to ensure that block's tree
/// is correctly detected depends on the type of tree. The maximum tree sizes
/// (for corresponding trunk base sizes) are supposedly:
///
/// Acacia:     12x10x12 (1x1)
/// Birch:      5x8x5 (1x1)
/// Dark oak:   12x11x12 (2x2)
/// Jungle:     16x32x16 (2x2), 5x13x5 (1x1)
/// Oak:        13x19x13 (1x1), 5x7x5 (1x1)
/// Spruce:     10x30x10 (2x2), 7x10x7 (1x1)
///
/// In order to (safely) detect the full extent of one tree, one should use
/// at least 2x those sizes in all directions if block is leaves, while shorter
/// search distance can be used for some tree types if block is log.
///
/// It would probably be a good idea to use a shared database accessible from
/// all functions needing this information, for memoization.
///
/// For now, the function classifies all trees in the entire WorldExcerpt.
fn prepare(excerpt: &WorldExcerpt) -> HashMap<BlockCoord, TreeBlockInfo> {
    let mut database = HashMap::new();
    let mut tree_id_counter = 0..;

    let (x_len, y_len, z_len) = excerpt.dim();
    let (x_len, y_len, z_len) = (x_len as i64, y_len as i64, z_len as i64);

    for x in 0..x_len {
        for y in 0..y_len {
            for z in 0..z_len {
                let coordinates = (x, y, z).into();
                if database.contains_key(&coordinates) {
                    continue;
                }
                if let Some(Block::Log(Log {
                    stripped: false, ..
                })) = excerpt.block_at(coordinates)
                {
                    // Find all related tree trunks
                    let log_group = find_connected_logs(excerpt, &coordinates);

                    let tree_id = tree_id_counter.next().unwrap();
                    for coordinates in log_group {
                        database.insert(
                            coordinates,
                            TreeBlockInfo {
                                tree_id,
                                parent: None,
                            },
                        );
                    }

                    // TODO do some parent finding magic
                    // * find "lowest" tree part
                    // * flood fill from there:
                    //  - may need some proper thought on ordering for that...
                }
            }
        }
    }
    // TODO Locate individual trees
    // * locate individual trunk/branch systems
    // * assign leaves to closest trunk/branch system (flood fill from the systems?)
    // * assign "parent/child" relationships from leaf to leaf to log to log
    // Goal:
    // Easy lookup location -> information about any tree block at that location

    database
}

/// Find all "connected" logs of the given material
fn find_connected_logs(excerpt: &WorldExcerpt, at: &BlockCoord) -> HashSet<BlockCoord> {
    let mut log_collection = HashSet::<BlockCoord>::new();
    let mut to_search = VecDeque::<BlockCoord>::new();

    if let Some(Block::Log(Log { material, .. })) = excerpt.block_at(*at) {
        to_search.push_back(*at);

        while let Some(coordinates) = to_search.pop_front() {
            if let Some(Block::Log(Log {
                material: material_found,
                ..
            })) = excerpt.block_at(coordinates)
            {
                // Only traverse logs of correct material
                if material_found != material {
                    continue;
                }

                // Add log to collection
                log_collection.insert(coordinates);

                // Add neighbour coordinates to search queue
                for neighbour_coordinates in neighbours_26(&coordinates) {
                    if !to_search.contains(&neighbour_coordinates)
                        && !log_collection.contains(&neighbour_coordinates)
                    {
                        to_search.push_back(neighbour_coordinates);
                    }
                }
            }
        }
    }

    log_collection
}

//NB Heavy reuse of find_connected_logs(). Could this be refactored to share code somehow?
/// Find all "connected" leaves of the given material
fn find_connected_leaves(excerpt: &WorldExcerpt, at: &BlockCoord) -> HashSet<BlockCoord> {
    let mut leaves_collection = HashSet::<BlockCoord>::new();
    let mut to_search = VecDeque::<BlockCoord>::new();

    if let Some(Block::Leaves { material, .. }) = excerpt.block_at(*at) {
        to_search.push_back(*at);

        while let Some(coordinates) = to_search.pop_front() {
            if let Some(Block::Leaves {
                material: material_found,
                ..
            }) = excerpt.block_at(coordinates)
            {
                // Only traverse logs of correct material
                if material_found != material {
                    continue;
                }

                // Add log to collection
                leaves_collection.insert(coordinates);

                // Add neighbour coordinates to search queue
                for neighbour_coordinates in neighbours_6(&coordinates) {
                    if !to_search.contains(&neighbour_coordinates)
                        && !leaves_collection.contains(&neighbour_coordinates)
                    {
                        to_search.push_back(neighbour_coordinates);
                    }
                }
            }
        }
    }

    leaves_collection
}

fn neighbours_4(at: &BlockCoord) -> Vec<BlockCoord> {
    vec![
        *at - (1, 0, 0).into(),
        *at - (0, 0, 1).into(),
        *at + (1, 0, 0).into(),
        *at + (0, 0, 1).into(),
    ]
}

fn neighbours_6(at: &BlockCoord) -> Vec<BlockCoord> {
    vec![
        *at - (1, 0, 0).into(),
        *at - (0, 1, 0).into(),
        *at - (0, 0, 1).into(),
        *at + (1, 0, 0).into(),
        *at + (0, 1, 0).into(),
        *at + (0, 0, 1).into(),
    ]
}

fn neighbours_26(at: &BlockCoord) -> Vec<BlockCoord> {
    let mut neighbours = Vec::with_capacity(26);

    for x in at.0 - 1..=at.0 + 1 {
        for y in at.1 - 1..=at.1 + 1 {
            for z in at.2 - 1..=at.2 + 1 {
                let neighbour_coordinates = (x, y, z).into();
                if neighbour_coordinates != *at {
                    neighbours.push(neighbour_coordinates);
                }
            }
        }
    }

    neighbours
}

/// Function for testing out tree finding; replaces tree logs with colourful concrete.
pub fn rainbow_trees(excerpt: &mut WorldExcerpt) {
    let mut tree_id_counter = 0..;

    let (x_len, y_len, z_len) = excerpt.dim();
    let (x_len, y_len, z_len) = (x_len as i64, y_len as i64, z_len as i64);

    for x in 0..x_len {
        for y in 0..y_len {
            for z in 0..z_len {
                let coordinates = (x, y, z).into();

                let tree = find_tree(excerpt, &coordinates);
                if tree.len() > 0 {
                    // Found a tree!
                    let tree_id = tree_id_counter.next().unwrap();

                    for coordinates in tree {
                        let colour = ((tree_id % 16) as i32).into();

                        match excerpt.block_at(coordinates) {
                            Some(Block::Log(_)) => {
                                excerpt.set_block_at(coordinates, Block::Concrete { colour });
                            }
                            Some(Block::Leaves { .. }) => {
                                excerpt.set_block_at(
                                    coordinates,
                                    Block::Glass {
                                        colour: Some(colour),
                                    },
                                );
                            }
                            Some(Block::Vines { .. }) => {
                                excerpt.set_block_at(coordinates, Block::Wool { colour });
                            }
                            _ => (),
                        }
                    }
                }
            }
        }
    }
}

pub fn _rainbow_trees_alternative_approach(excerpt: &mut WorldExcerpt) {
    let database = prepare(excerpt);

    for (coordinates, TreeBlockInfo { tree_id, .. }) in &database {
        let colour = ((tree_id % 16) as i32).into();
        match excerpt.block_at(*coordinates) {
            Some(Block::Log(_)) => {
                excerpt.set_block_at(*coordinates, Block::Concrete { colour });
            }
            Some(Block::Leaves { .. }) => {
                excerpt.set_block_at(
                    *coordinates,
                    Block::Glass {
                        colour: Some(colour),
                    },
                );
            }
            Some(Block::Vines { .. }) => {
                excerpt.set_block_at(*coordinates, Block::Wool { colour });
            }
            _ => (),
        }
    }
}

/// Traverses Leaves blocks in search of the closest corresponding Log block.
/// If already at a Log, return that Log's coordinates.
fn find_nearest_connected_log(excerpt: &WorldExcerpt, at: &BlockCoord) -> Option<BlockCoord> {
    match excerpt.block_at(*at) {
        Some(Block::Log(_)) => Some(*at),
        Some(Block::Leaves {
            material: leaves_material,
            ..
        }) => {
            let log_material = WoodMaterial::try_from(*leaves_material).unwrap();
            let mut leaves_collection = HashSet::<BlockCoord>::new();
            let mut to_search = VecDeque::<BlockCoord>::new();
            to_search.push_back(*at);

            // Search for the corresponding log
            while let Some(coordinates) = to_search.pop_front() {
                match excerpt.block_at(coordinates) {
                    Some(Block::Log(Log {
                        material: material_found,
                        ..
                    })) => {
                        if *material_found == log_material {
                            return Some(coordinates);
                        }
                    }
                    Some(Block::Leaves {
                        material: material_found,
                        ..
                    }) => {
                        // Only traverse leaves of correct material
                        if material_found != leaves_material {
                            continue;
                        }

                        // Add leaves to collection
                        leaves_collection.insert(coordinates);

                        // Add neighbour coordinates to search queue
                        for neighbour_coordinates in neighbours_6(&coordinates) {
                            if !to_search.contains(&neighbour_coordinates)
                                && !leaves_collection.contains(&neighbour_coordinates)
                            {
                                to_search.push_back(neighbour_coordinates);
                            }
                        }
                    }
                    _ => (),
                }
            }

            // Did not find a log during the search
            None
        }

        // Did not start on Leaves or Log
        _ => None,
    }
}

fn find_tree(excerpt: &WorldExcerpt, at: &BlockCoord) -> HashSet<BlockCoord> {
    let mut tree_block_collection = HashSet::<BlockCoord>::new();

    match find_nearest_connected_log(excerpt, at) {
        None => find_connected_leaves(excerpt, at),
        Some(start_log_coordinates) => {
            // Find the material types of the tree
            let log_material = match excerpt.block_at(start_log_coordinates) {
                Some(Block::Log(Log { material, .. })) => material,
                _ => unreachable!(),
            };
            let leaves_material = LeavesMaterial::try_from(*log_material).unwrap();

            // Find the connected logs
            let log_coordinates = find_connected_logs(excerpt, &start_log_coordinates);

            // Structures needed for search algorithm
            let mut to_search = VecDeque::<BlockCoord>::new();
            let mut found_nodes = HashMap::<BlockCoord, TreeSearchInfo>::new();

            // Include the log block in the output, as well as in the found_nodes register
            for coordinates in &log_coordinates {
                tree_block_collection.insert(*coordinates);
                found_nodes.insert(
                    *coordinates,
                    TreeSearchInfo {
                        parent: None,
                        distance: 0,
                        known_foreign: false,
                        handled: true,
                    },
                );
            }

            // Add the neighbours of logs (but not the logs themselves) to the search queue,
            // along with node info
            for coordinates in &log_coordinates {
                for neighbour_coordinates in neighbours_26(&coordinates) {
                    if !to_search.contains(&neighbour_coordinates)
                        && !tree_block_collection.contains(&neighbour_coordinates)
                    {
                        to_search.push_back(neighbour_coordinates);
                        found_nodes.insert(
                            neighbour_coordinates,
                            TreeSearchInfo {
                                parent: Some(*coordinates),
                                distance: 1,
                                known_foreign: false,
                                handled: false,
                            },
                        );
                    }
                }
            }

            // The search itself
            while let Some(coordinates) = to_search.pop_front() {
                match excerpt.block_at(coordinates) {
                    Some(Block::Log(Log {
                        material: material_found,
                        ..
                    })) => {
                        // Backtrace from logs of the correct type,
                        // that were not in the original set.
                        if material_found == log_material
                            && !tree_block_collection.contains(&coordinates)
                        {
                            // We found a log from a different tree
                            let foreign_blocks = backtrace(&coordinates, 0, &found_nodes);
                            // Update list of found nodes, and remove blocks from output
                            for (coordinates, foreign_block_info) in &foreign_blocks {
                                found_nodes.insert(*coordinates, *foreign_block_info);
                                tree_block_collection.remove(coordinates);
                            }
                        }
                    }
                    Some(Block::Leaves {
                        material: material_found,
                        ..
                    }) => {
                        // Only handle the correct type of tree
                        if *material_found != leaves_material {
                            continue;
                        }

                        let info = match found_nodes.get(&coordinates) {
                            None => unreachable!(),
                            Some(&info) => info,
                        };

                        if !info.handled {
                            // Add node
                            tree_block_collection.insert(coordinates);

                            // Traverse further
                            for neighbour_coordinates in neighbours_6(&coordinates) {
                                if !to_search.contains(&neighbour_coordinates)
                                    && !found_nodes.contains_key(&neighbour_coordinates)
                                {
                                    to_search.push_back(neighbour_coordinates);
                                    found_nodes.insert(
                                        neighbour_coordinates,
                                        TreeSearchInfo {
                                            parent: Some(coordinates),
                                            distance: info.distance + 1,
                                            known_foreign: false,
                                            handled: false,
                                        },
                                    );
                                }
                            }
                        } else if info.known_foreign {
                            // We found leaves from a different tree
                            let foreign_blocks =
                                backtrace(&coordinates, info.distance, &found_nodes);
                            // Update list of found nodes, and remove blocks from output
                            for (coordinates, foreign_block_info) in &foreign_blocks {
                                found_nodes.insert(*coordinates, *foreign_block_info);
                                tree_block_collection.remove(coordinates);
                            }
                        } else {
                            // Skip if not known to be foreign
                            continue;
                        }
                    }
                    _ => (),
                }
            }

            // Handle Vines
            let mut vines = HashSet::<BlockCoord>::new();
            for coordinates in &tree_block_collection {
                for neighbour_coordinates in neighbours_4(&coordinates) {
                    for y in (0..=neighbour_coordinates.1).rev() {
                        let block_coordinates = (
                            neighbour_coordinates.0,
                            y,
                            neighbour_coordinates.2
                        ).into();

                        if tree_block_collection.contains(&block_coordinates) {
                            break;
                        }

                        match excerpt.block_at(block_coordinates) {
                            Some(Block::Vines(_)) => vines.insert(block_coordinates),
                            _ => break,
                        };
                    }
                }
            }

            // TODO Handle mushrooms growing on trees. (Is that in swamp biomes only?)
            tree_block_collection.union(&vines).cloned().collect()
        }
    }
}

fn backtrace(
    from: &BlockCoord,
    from_distance: usize,
    through: &HashMap<BlockCoord, TreeSearchInfo>,
) -> HashMap<BlockCoord, TreeSearchInfo> {
    let mut to_search = VecDeque::<BlockCoord>::new();
    let mut found_nodes = HashMap::<BlockCoord, TreeSearchInfo>::new();
    let mut foreign = HashMap::<BlockCoord, TreeSearchInfo>::new();

    // Add the node from which we start the search
    to_search.push_back(*from);
    found_nodes.insert(
        *from,
        TreeSearchInfo {
            parent: None, // FIXME get parent from `through`?
            distance: from_distance,
            known_foreign: true,
            handled: false,
        },
    );

    while let Some(coordinates) = to_search.pop_front() {
        let foreign_info = match found_nodes.get(&coordinates) {
            None => unreachable!(),
            Some(&info) => info,
        };

        if let Some(TreeSearchInfo {
            distance: tree_distance,
            known_foreign: tree_known_foreign,
            ..
        }) = through.get(&coordinates)
        {
            // If the distance to the foreign tree gets larger than the
            // distance to "our" tree, then this block belongs to "our" tree.
            if foreign_info.distance >= *tree_distance {
                continue;
            }

            // Do not traverse blocks known to belong to the foreign tree,
            // other than the very first one
            if coordinates != *from && *tree_known_foreign {
                continue;
            }

            // Add to foreign
            foreign.insert(
                coordinates,
                TreeSearchInfo {
                    parent: foreign_info.parent,
                    distance: foreign_info.distance,
                    known_foreign: true,
                    handled: true,
                },
            );

            // Add neighbours
            for neighbour_coordinates in neighbours_6(&coordinates) {
                if !to_search.contains(&neighbour_coordinates)
                    && !found_nodes.contains_key(&neighbour_coordinates)
                {
                    to_search.push_back(neighbour_coordinates);
                    found_nodes.insert(
                        neighbour_coordinates,
                        TreeSearchInfo {
                            parent: Some(coordinates),
                            distance: foreign_info.distance + 1,
                            known_foreign: false,
                            handled: false,
                        },
                    );
                }
            }
        }
    }

    foreign
}

#[derive(Clone, Copy)]
struct TreeSearchInfo {
    parent: Option<BlockCoord>,
    distance: usize,
    known_foreign: bool,
    handled: bool,
}