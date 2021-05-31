use crate::block_palette::BlockPalette;
use crate::build_area::BuildArea;
use mcprogedit;
use mcprogedit::block::{Block, Flower};
use mcprogedit::coordinates::BlockCoord;
use mcprogedit::positioning::Surface5;
use mcprogedit::world_excerpt::WorldExcerpt;
use std::cmp::max;
use std::collections::HashSet;

pub fn _build_rock(
    excerpt: &WorldExcerpt,
    build_area: &BuildArea,
    _palette: &BlockPalette,
) -> Option<WorldExcerpt> {
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

pub fn build_house(
    excerpt: &WorldExcerpt,
    build_area: &BuildArea,
    palette: &BlockPalette,
) -> Option<WorldExcerpt> {
    const WALL_HEIGHT: usize = 3;

    // WorldExcerpt for holding the additions/changes to the world
    let (x_len, y_len, z_len) = excerpt.dim();
    let mut output = WorldExcerpt::new(x_len, y_len, z_len);

    // Find the coordinates inside and outside of the plot itself
    let mut buildable = build_area.buildable_coordinates();
    let mut not_buildable = build_area.not_buildable_coordinates();

    // Find the circumferal blocks (that are still inside the build area)
    let mut buildable_edge = build_area.buildable_edge_coordinates();

    // Find the road blocks bordering the buildable area
    let mut road_along_buildable = build_area.road_along_buildable_coordinates();

    // Get height map for the area
    let mut height_map = excerpt.height_map();

    // Update the height map to not include foilage.
    for x in 0..x_len as usize {
        for z in 0..z_len as usize {
            let y = height_map.height_at((x, z)).unwrap_or(y_len as u32);

            for y in (0..y).rev() {
                if let Some(block) = excerpt.block_at((x as i64, y as i64, z as i64).into()) {
                    if !block.is_foilage() {
                        height_map.set_height((x, z), y as u32 + 1);
                        break;
                    }
                }
            }
        }
    }

    // "Clean up" the build area a bit, by removing weird outliers.
    let mut changes = 1;
    while changes > 0 {
        changes = 0;
        let mut to_remove = Vec::new();

        for coordinates in &buildable_edge {
            let mut outside_neighbours = 0;
            let mut road_accessible_neighbours = 0;
            for x in coordinates.0 - 1..=coordinates.0 + 1 {
                for z in coordinates.1 - 1..=coordinates.1 + 1 {
                    if not_buildable.contains(&(x, z)) {
                        outside_neighbours += 1;
                    }
                    if road_along_buildable.contains(&(x, z)) {
                        road_accessible_neighbours += 1;
                    }
                }
            }
            if outside_neighbours > 5 {
                changes += 1;
                buildable.remove(coordinates);
                to_remove.push(*coordinates);
                not_buildable.insert(*coordinates);
                if road_accessible_neighbours > 0 {
                    road_along_buildable.insert(*coordinates);
                }
            }
        }

        for coordinates in to_remove {
            buildable_edge.remove(&coordinates);
        }
    }

    // Find average road side y along plot
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

    // In order to avoid surprises, replace lava at dangerous locations with obsidian..
    for x in 0..x_len {
        for y in road_y_average - 10..y_len {
            for z in 0..z_len {
                let coordinates = BlockCoord(x as i64, y as i64, z as i64);
                if let Some(Block::LavaSource) = excerpt.block_at(coordinates) {
                    output.set_block_at(coordinates, Block::Obsidian);
                }
                if let Some(Block::Lava { .. }) = excerpt.block_at(coordinates) {
                    output.set_block_at(coordinates, Block::Obsidian);
                }
            }
        }
    }

    // Build foundations on plot up to average road height
    for (x, z) in &buildable_edge {
        let terrain_y = height_map.height_at((*x, *z)).unwrap();
        // Build foundations up to floor block level
        for y in terrain_y as i64..road_y_average as i64 {
            output.set_block_at(BlockCoord(*x as i64, y, *z as i64), palette.foundation.clone());
        }
        // Remove terrain from floor block level and up
        for y in road_y_average as i64..=terrain_y as i64 {
            output.set_block_at(BlockCoord(*x as i64, y, *z as i64), Block::Air);
        }
        // Build foundations at floor block level
        output.set_block_at(
            BlockCoord(*x as i64, road_y_average as i64, *z as i64),
            palette.foundation.clone(),
        );
    }

    // Build floor
    for (x, z) in &buildable {
        if !buildable_edge.contains(&(*x, *z)) {
            output.set_block_at(
                BlockCoord(*x as i64, road_y_average as i64, *z as i64),
                palette.floor.clone(),
            );
            for y in (road_y_average + 1)..=(road_y_average + WALL_HEIGHT) {
                output.set_block_at(BlockCoord(*x as i64, y as i64, *z as i64), Block::Air);
            }
        }
    }

    // Build wall along plot edge
    for (x, z) in &buildable_edge {
        for y in (road_y_average + 1)..=(road_y_average + WALL_HEIGHT) {
            output.set_block_at(BlockCoord(*x as i64, y as i64, *z as i64), palette.wall.clone());
        }
    }

    // Put door in wall along plot edge facing road (mind also y positions)
    // TODO Put a block or some stairs down outside door, if needed.
    let mut door_placed = false;
    let mut door_location = None;

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
                // Make sure door has air outside.
                output.set_block_at(
                    BlockCoord(*x as i64, road_y_average as i64 + 1, *z as i64 - 1),
                    Block::Air,
                );
                output.set_block_at(
                    BlockCoord(*x as i64, road_y_average as i64 + 2, *z as i64 - 1),
                    Block::Air,
                );
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
                door_location = Some((*x, *z));
                break;
            }

            // South is road, north is inside
            if road_along_buildable.contains(&south_coordinates)
                && !buildable_edge.contains(&north_coordinates)
                && buildable.contains(&north_coordinates)
            {
                // Make sure door has air outside.
                output.set_block_at(
                    BlockCoord(*x as i64, road_y_average as i64 + 1, *z as i64 + 1),
                    Block::Air,
                );
                output.set_block_at(
                    BlockCoord(*x as i64, road_y_average as i64 + 2, *z as i64 + 1),
                    Block::Air,
                );
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
                door_location = Some((*x, *z));
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
                // Make sure door has air outside.
                output.set_block_at(
                    BlockCoord(*x as i64 + 1, road_y_average as i64 + 1, *z as i64),
                    Block::Air,
                );
                output.set_block_at(
                    BlockCoord(*x as i64 + 1, road_y_average as i64 + 2, *z as i64),
                    Block::Air,
                );
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
                door_location = Some((*x, *z));
                break;
            }

            // West is road, east is inside
            if road_along_buildable.contains(&west_coordinates)
                && !buildable_edge.contains(&east_coordinates)
                && buildable.contains(&east_coordinates)
            {
                // Make sure door has air outside.
                output.set_block_at(
                    BlockCoord(*x as i64 - 1, road_y_average as i64 + 1, *z as i64),
                    Block::Air,
                );
                output.set_block_at(
                    BlockCoord(*x as i64 - 1, road_y_average as i64 + 2, *z as i64),
                    Block::Air,
                );
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
                door_location = Some((*x, *z));
                break;
            }
        }
    }
    if !door_placed {
        // TODO Consider trying a different strategy before giving up.
        println!("Unable to find a suitable location for the door!");
        return None;
    }

    // Find some window locations where we know the wall is not blocked (i.e. along roads.)
    let mut window_locations = Vec::new();
    for (x, z) in &buildable_edge {
        // We cannot put windows where we already put a door.
        if door_location == Some((*x, *z)) {
            continue;
        }

        // We need a wall block on either side, then outside on one side and inside on another,
        // and not door adjacent to the window location.
        if buildable_edge.contains(&(*x - 1, *z))
        && door_location != Some((*x - 1, *z))
        && buildable_edge.contains(&(*x + 1, *z))
        && door_location != Some((*x + 1, *z))
        && !buildable_edge.contains(&(*x, *z - 1))
        && !buildable_edge.contains(&(*x, *z + 1))
        {
            if buildable.contains(&(*x, *z - 1))
            && road_along_buildable.contains(&(*x, *z + 1))
            || buildable.contains(&(*x, *z + 1))
            && road_along_buildable.contains(&(*x, *z - 1))
            {
                window_locations.push((*x, *z));
            }
        }

        // Same as above, but in the other orientation.
        if buildable_edge.contains(&(*x, *z - 1))
        && door_location != Some((*x, *z - 1))
        && buildable_edge.contains(&(*x, *z + 1))
        && door_location != Some((*x, *z + 1))
        && !buildable_edge.contains(&(*x - 1, *z))
        && !buildable_edge.contains(&(*x + 1, *z))
        {
            if buildable.contains(&(*x - 1, *z))
            && road_along_buildable.contains(&(*x + 1, *z))
            || buildable.contains(&(*x + 1, *z))
            && road_along_buildable.contains(&(*x - 1, *z))
            {
                window_locations.push((*x, *z));
            }
        }
    }

    // Build windows at (at least some) of the locations found
    for (x, z) in &window_locations {
        output.set_block_at(
            BlockCoord(*x as i64, road_y_average as i64 + 2, *z as i64),
            palette.flat_window.clone(),
        );
    }

    // Put down some torches
    for (index, (x, z)) in buildable_edge.iter().enumerate() {
        let y = if door_location == Some((*x, *z)) || window_locations.contains(&(*x, *z)) {
            // Do not place torch attached to the door, put it above the door instead.
            // Same strategy used for windows.
            road_y_average as i64 + 3
        } else {
            road_y_average as i64 + 2
        };

        let west = (*x + 1, *z);
        let east = (*x - 1, *z);
        let north = (*x, *z + 1);
        let south = (*x, *z - 1);

        // Build torch outside?
        if index % 6 == 0 || door_location == Some((*x, *z)) {
            if road_along_buildable.contains(&west) {
                output.set_block_at(
                    BlockCoord(west.0 as i64, y, west.1 as i64),
                    Block::Torch { attached: Surface5::West },
                );
            } else if road_along_buildable.contains(&east) {
                output.set_block_at(
                    BlockCoord(east.0 as i64, y, east.1 as i64),
                    Block::Torch { attached: Surface5::East },
                );
            } else if road_along_buildable.contains(&north) {
                output.set_block_at(
                    BlockCoord(north.0 as i64, y, north.1 as i64),
                    Block::Torch { attached: Surface5::North },
                );
            } else if road_along_buildable.contains(&south) {
                output.set_block_at(
                    BlockCoord(south.0 as i64, y, south.1 as i64),
                    Block::Torch { attached: Surface5::South },
                );
            }
        }

        // Build torch inside?
        if index % 4 == 0 {
            if buildable.contains(&west) && ! buildable_edge.contains(&west) {
                output.set_block_at(
                    BlockCoord(west.0 as i64, y, west.1 as i64),
                    Block::Torch { attached: Surface5::West },
                );
            } else if buildable.contains(&east) && ! buildable_edge.contains(&east) {
                output.set_block_at(
                    BlockCoord(east.0 as i64, y, east.1 as i64),
                    Block::Torch { attached: Surface5::East },
                );
            } else if buildable.contains(&north) && ! buildable_edge.contains(&north) {
                output.set_block_at(
                    BlockCoord(north.0 as i64, y, north.1 as i64),
                    Block::Torch { attached: Surface5::North },
                );
            } else if buildable.contains(&south) && ! buildable_edge.contains(&south) {
                output.set_block_at(
                    BlockCoord(south.0 as i64, y, south.1 as i64),
                    Block::Torch { attached: Surface5::South },
                );
            }
        }
    }

    if !palette.flowers.is_empty() {
        for (index, (x, z)) in road_along_buildable.iter().enumerate(){
            // Don't put anything down most of the time.
            if index % 3 != 0 {
                continue;
            }

            let terrain_y = height_map.height_at((*x, *z)).unwrap();

            let ground_location = BlockCoord(*x as i64, terrain_y as i64 - 1, *z as i64);
            let first_block = ground_location + BlockCoord(0, 1, 0);
            let second_block = ground_location + BlockCoord(0, 2, 0);

            // Do not put detailing down if something else has been put there before
            if output.block_at(ground_location) != Some(&Block::None)
            || output.block_at(first_block) != Some(&Block::None)
            || output.block_at(second_block) != Some(&Block::None)
            {
                continue;
            }

            match excerpt.block_at(ground_location) {
                Some(Block::GrassBlock) => {
                    let flower_index = index % max(8, palette.flowers.len() + 2);
                    if flower_index < palette.flowers.len() {
                        // Below flower
                        match index % 3 {
                            0 | 1 => output.set_block_at(ground_location, Block::CoarseDirt),
                            _ => output.set_block_at(ground_location, Block::Podzol),
                        }
                        // Bottom part
                        output.set_block_at(first_block, Block::Flower(palette.flowers[flower_index]));

                        // Top part
                        match palette.flowers[flower_index] {
                            Flower::LilacBottom => {
                                output.set_block_at(second_block, Block::Flower(Flower::LilacTop));
                            }
                            Flower::PeonyBottom => {
                                output.set_block_at(second_block, Block::Flower(Flower::PeonyTop));
                            }
                            Flower::RoseBushBottom => {
                                output.set_block_at(second_block, Block::Flower(Flower::RoseBushTop));
                            }
                            Flower::SunflowerBottom => {
                                output.set_block_at(second_block, Block::Flower(Flower::SunflowerTop));
                            }
                            _ => (),
                        }
                    } else {
                        // TODO Maybe consider something else?
                    }
                }
                _ => (),
            }
        }
    }
    // TODO Put detailing outside:
    //      Flowers. Flower beds. Vines. Flower pots. Along outer wall.

    // TODO Put furniture inside:
    //      Bed. Workbench. Furnace. Flower pots. Chest? Chairs? Tables? Pictures?

    // Put roof on top
    let mut available_to_roof = buildable.clone();
    let mut unavailable_to_roof = not_buildable.clone();
    let mut y = road_y_average as i64 + WALL_HEIGHT as i64 + 1;

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
            output.set_block_at(BlockCoord(x as i64, y, z as i64), palette.roof.clone());
            available_to_roof.remove(&(x, z));
            unavailable_to_roof.insert((x, z));
        }

        // Increase y for next iteration
        y += 1;
    }

    // Return our additions to the world
    Some(output)
}
