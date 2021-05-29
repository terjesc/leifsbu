use crate::build_area::BuildArea;
use mcprogedit;
use mcprogedit::block::Block;
use mcprogedit::coordinates::BlockCoord;
use mcprogedit::world_excerpt::WorldExcerpt;
use std::collections::HashSet;

pub fn build_rock(excerpt: &WorldExcerpt, build_area: &BuildArea) -> Option<WorldExcerpt> {
    let height_map = excerpt.height_map();

    let (x_len, y_len, z_len) = excerpt.dim();
    let mut output = WorldExcerpt::new(x_len, y_len, z_len);

    for x in 0..x_len {
        for z in 0..z_len {
            if let Some(designation) = build_area.designation_at((x as usize, z as usize)) {
                if designation.is_buildable() {
                    // Get terrain height
                    if let Some(y) = height_map.height_at((x as usize, z as usize)) {
                        // Build something on the terrain
                        for y in y..y + 5 {
                            output.set_block_at(
                                BlockCoord(x as i64, y as i64, z as i64),
                                Block::Stone,
                            );
                        }
                    }
                }
            }
        }
    }

    Some(output)
}

pub fn build_house(excerpt: &WorldExcerpt, build_area: &BuildArea) -> Option<WorldExcerpt> {
    const FOUNDATION_BLOCK: Block = Block::Cobblestone;
    const FLOOR_BLOCK: Block = Block::dark_oak_planks();
    const WALL_BLOCK: Block = Block::oak_planks();
    const ROOF_BLOCK: Block = Block::BrickBlock;

    const WALL_HEIGHT: usize = 3;

    // WorldExcerpt for holding the additions/changes to the world
    let (x_len, y_len, z_len) = excerpt.dim();
    let mut output = WorldExcerpt::new(x_len, y_len, z_len);

    let buildable = build_area.buildable_coordinates();
    let not_buildable = build_area.not_buildable_coordinates();

    // Get height map for the area
    let height_map = excerpt.height_map();

    // Find the circumferal blocks (that are still inside the build area)
    let buildable_edge = build_area.buildable_edge_coordinates();

    // TODO Convert from buildable_edge to a nicer circumference,
    //      by removing weird outliers.
    // TODO Then also make sure to update the interior area.
    // TODO And also make sure to handle roads correctly (as the area percieved
    //      as road may now be larger than before)

    // Find average road y along plot
    let road_along_buildable = build_area.road_along_buildable_coordinates();
    let road_y_values: Vec<usize> = road_along_buildable
        .iter()
        .filter_map(|(x, z)| height_map.height_at((*x, *z)))
        .map(|y| y as usize)
        .collect();
    if road_y_values.is_empty() {
        // Abort house building if we cannot find any roads to attach to.
        return None;
    }
    let road_y_average: usize = road_y_values.iter().sum::<usize>() / road_y_values.len();

    // TODO If there is lava close to (wooden) house placement, abort (return None.)
    // ALTERNATIVELY just replace lava with obsidian

    // Build foundations on plot up to average road height
    for (x, z) in &buildable_edge {
        let terrain_y = height_map.height_at((*x, *z)).unwrap();
        // Build foundations up to floor block level
        for y in terrain_y as i64..road_y_average as i64 {
            output.set_block_at(BlockCoord(*x as i64, y, *z as i64), FOUNDATION_BLOCK);
        }
        // Remove terrain from floor block level and up
        for y in road_y_average as i64..=terrain_y as i64 {
            output.set_block_at(BlockCoord(*x as i64, y, *z as i64), Block::Air);
        }
        // Build foundations at floor block level
        output.set_block_at(
            BlockCoord(*x as i64, road_y_average as i64, *z as i64),
            FOUNDATION_BLOCK,
        );
    }

    // Build floor
    for (x, z) in &buildable {
        if !buildable_edge.contains(&(*x, *z)) {
            output.set_block_at(
                BlockCoord(*x as i64, road_y_average as i64, *z as i64),
                FLOOR_BLOCK,
            );
            for y in (road_y_average + 1)..=(road_y_average + WALL_HEIGHT) {
                output.set_block_at(BlockCoord(*x as i64, y as i64, *z as i64), Block::Air);
            }
        }
    }

    // Build wall along plot edge
    for (x, z) in &buildable_edge {
        for y in (road_y_average + 1)..=(road_y_average + WALL_HEIGHT) {
            output.set_block_at(BlockCoord(*x as i64, y as i64, *z as i64), WALL_BLOCK);
        }
    }

    // Put door in wall along plot edge facing road (mind also y positions)
    // TODO Put in a little more effort, in order to place doors on
    //      diagonals as well, if straight edges cannot be found.
    // TODO Put some stairs down outside door, if needed.
    // TODO Put down door only if higher than road.
    // TODO Pick a more "central" location for the door, along the wall.
    let mut door_placed = false;
    for (x, z) in &buildable_edge {
        let north_coordinates = (*x, *z - 1);
        let west_coordinates = (*x - 1, *z);
        let south_coordinates = (*x, *z + 1);
        let east_coordinates = (*x + 1, *z);

        // Must have wall to both sides
        if buildable_edge.contains(&east_coordinates) && buildable_edge.contains(&west_coordinates)
        {
            // North is road, south is inside
            if road_along_buildable.contains(&north_coordinates)
                && !buildable_edge.contains(&south_coordinates)
                && buildable.contains(&south_coordinates)
            {
                // Put door hinged on south side
                output.set_block_at(
                    BlockCoord(*x as i64, road_y_average as i64 + 1, *z as i64),
                    Block::Door(mcprogedit::block::Door {
                        material: mcprogedit::material::DoorMaterial::Oak,
                        facing: mcprogedit::positioning::Surface4::South,
                        half: mcprogedit::block::DoorHalf::Lower,
                        hinged_at: mcprogedit::block::Hinge::Left,
                        open: false,
                    }),
                );
                output.set_block_at(
                    BlockCoord(*x as i64, road_y_average as i64 + 2, *z as i64),
                    Block::Door(mcprogedit::block::Door {
                        material: mcprogedit::material::DoorMaterial::Oak,
                        facing: mcprogedit::positioning::Surface4::South,
                        half: mcprogedit::block::DoorHalf::Upper,
                        hinged_at: mcprogedit::block::Hinge::Left,
                        open: false,
                    }),
                );
                door_placed = true;
                break;
            }

            // South is road, north is inside
            if road_along_buildable.contains(&south_coordinates)
                && !buildable_edge.contains(&north_coordinates)
                && buildable.contains(&north_coordinates)
            {
                // Put door hinged on north side
                output.set_block_at(
                    BlockCoord(*x as i64, road_y_average as i64 + 1, *z as i64),
                    Block::Door(mcprogedit::block::Door {
                        material: mcprogedit::material::DoorMaterial::Oak,
                        facing: mcprogedit::positioning::Surface4::North,
                        half: mcprogedit::block::DoorHalf::Lower,
                        hinged_at: mcprogedit::block::Hinge::Left,
                        open: false,
                    }),
                );
                output.set_block_at(
                    BlockCoord(*x as i64, road_y_average as i64 + 2, *z as i64),
                    Block::Door(mcprogedit::block::Door {
                        material: mcprogedit::material::DoorMaterial::Oak,
                        facing: mcprogedit::positioning::Surface4::North,
                        half: mcprogedit::block::DoorHalf::Upper,
                        hinged_at: mcprogedit::block::Hinge::Left,
                        open: false,
                    }),
                );
                door_placed = true;
                break;
            }
        }

        // Must have wall to both sides
        if buildable_edge.contains(&north_coordinates)
            && buildable_edge.contains(&south_coordinates)
        {
            // East is road, west is inside
            if road_along_buildable.contains(&east_coordinates)
                && !buildable_edge.contains(&west_coordinates)
                && buildable.contains(&west_coordinates)
            {
                // Put door hinged on west side
                output.set_block_at(
                    BlockCoord(*x as i64, road_y_average as i64 + 1, *z as i64),
                    Block::Door(mcprogedit::block::Door {
                        material: mcprogedit::material::DoorMaterial::Oak,
                        facing: mcprogedit::positioning::Surface4::West,
                        half: mcprogedit::block::DoorHalf::Lower,
                        hinged_at: mcprogedit::block::Hinge::Left,
                        open: false,
                    }),
                );
                output.set_block_at(
                    BlockCoord(*x as i64, road_y_average as i64 + 2, *z as i64),
                    Block::Door(mcprogedit::block::Door {
                        material: mcprogedit::material::DoorMaterial::Oak,
                        facing: mcprogedit::positioning::Surface4::West,
                        half: mcprogedit::block::DoorHalf::Upper,
                        hinged_at: mcprogedit::block::Hinge::Left,
                        open: false,
                    }),
                );
                door_placed = true;
                break;
            }

            // West is road, east is inside
            if road_along_buildable.contains(&west_coordinates)
                && !buildable_edge.contains(&east_coordinates)
                && buildable.contains(&east_coordinates)
            {
                // Put door hinged on east side
                output.set_block_at(
                    BlockCoord(*x as i64, road_y_average as i64 + 1, *z as i64),
                    Block::Door(mcprogedit::block::Door {
                        material: mcprogedit::material::DoorMaterial::Oak,
                        facing: mcprogedit::positioning::Surface4::East,
                        half: mcprogedit::block::DoorHalf::Lower,
                        hinged_at: mcprogedit::block::Hinge::Left,
                        open: false,
                    }),
                );
                output.set_block_at(
                    BlockCoord(*x as i64, road_y_average as i64 + 2, *z as i64),
                    Block::Door(mcprogedit::block::Door {
                        material: mcprogedit::material::DoorMaterial::Oak,
                        facing: mcprogedit::positioning::Surface4::East,
                        half: mcprogedit::block::DoorHalf::Upper,
                        hinged_at: mcprogedit::block::Hinge::Left,
                        open: false,
                    }),
                );
                door_placed = true;
                break;
            }
        }
    }
    if !door_placed {
        println!("Unable to find a suitable location for the door!");
        return None;
    }

    // Put roof on top
    let mut available_to_roof: HashSet<(usize, usize)> = buildable.into_iter().collect();
    let mut unavailable_to_roof: HashSet<(usize, usize)> = not_buildable.into_iter().collect();
    let mut y = road_y_average as i64 + 4;

    while !available_to_roof.is_empty() {
        // Find everything in available_to_roof that is neighbour to unavailable_to_roof
        let mut current_roof_set = HashSet::new();
        for (x, z) in &available_to_roof {
            if unavailable_to_roof.contains(&(*x - 1, *z))
                || unavailable_to_roof.contains(&(*x + 1, *z))
                || unavailable_to_roof.contains(&(*x, *z - 1))
                || unavailable_to_roof.contains(&(*x, *z + 1))
            {
                current_roof_set.insert((*x, *z));
            }
        }

        // Build roof at the found locations, and move from available to unavailable
        for (x, z) in current_roof_set.drain() {
            output.set_block_at(BlockCoord(x as i64, y, z as i64), ROOF_BLOCK);
            available_to_roof.remove(&(x, z));
            unavailable_to_roof.insert((x, z));
        }

        // Increase y for next iteration
        y += 1;
    }

    // TODO Put windows in
    // Window is OK if in wall, and in two opposite cardinal directions there is Air (or None),
    // and in the two remaining cardinal directions there is Wall (or Window).

    // NB at this stage, it should have reached a minimal viable state

    // TODO Put torch by door, and/or other places along outer walls

    // NB at this stage, it should almost be "OK"

    // TODO Put detailing outside:
    //      Torches. Flowers. Flower beds. Vines. Flower pots. Along outer wall.

    // NB at this stage, it should start to look "decent"

    // TODO Put furniture inside:
    //      Bed. Workbench. Furnace. Torches. Flower pots. Chest? Chairs? Tables? Pictures?

    // NB at this stage, it should be fairly OK to ship (BTW, did you handle the lava?)

    // Return our additions to the world
    Some(output)
}
