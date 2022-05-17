use crate::block_palette::BlockPalette;
use crate::build_area::BuildArea;
use crate::geometry;
use crate::geometry::{RawEdge2d};
use crate::line::line;

use log::{trace, warn};
use mcprogedit::block::{Block, Flower};
use mcprogedit::coordinates::{BlockColumnCoord, BlockCoord};
use mcprogedit::positioning::{Surface4, Surface5};
use mcprogedit::world_excerpt::WorldExcerpt;
use std::cmp::{max, min};
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
) -> Option <WorldExcerpt> {

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
    let mut height_map = excerpt.ground_height_map();

    /*
    // FIXME remove: Mark an outline of "buildable area".
    for (x, z) in &buildable_edge {
        let y = height_map.height_at((*x, *z)).unwrap_or(255);
        let coordinates = BlockCoord(*x as i64, y as i64, *z as i64);
        output.set_block_at(coordinates, Block::Glass { colour: Some(mcprogedit::colour::Colour::Red) });
    }
    */

    // TODO exclude areas too thin, not contiguous, etc. (assign road or yard to thinned out parts)
    let mut buildable_interior: HashSet<(usize, usize)> = buildable.difference(&buildable_edge).copied().collect();

    // Remove from buildable_interior too thin portions. Iteratively remove from buildable_interior
    // any cell which has two or less neighbouring interior cells, in the 8-neighbourhood.
    let mut changes = 1;
    while changes > 0 {
        changes = 0;
        let mut to_remove = Vec::new();

        for coordinates in &buildable_interior {
            let mut interior_neighbours_count = 0;
            for x in coordinates.0 - 1..=coordinates.0 + 1 {
                for z in coordinates.1 - 1..=coordinates.1 + 1 {
                    if *coordinates != (x, z) && buildable_interior.contains(&(x, z)) {
                        interior_neighbours_count += 1;
                    }
                }
            }
            if interior_neighbours_count <= 2 {
                changes += 1;
                to_remove.push(*coordinates);
            }
        }

        for coordinates in to_remove {
            buildable_interior.remove(&coordinates);
        }
    }

    // Don't bother if the interior area of the building is less than 9 m²
    if buildable_interior.len() < 9 {
        trace!("Building would have less than 9 m² interior; aborting.");
        return None;
    }

/*
    // FIXME remove: Mark interior.
    for (x, z) in &buildable_interior {
        let y = height_map.height_at((*x, *z)).unwrap_or(255);
        let coordinates = BlockCoord(*x as i64, y as i64, *z as i64);
        output.set_block_at(coordinates, Block::Glass { colour: Some(mcprogedit::colour::Colour::Yellow) });
    }
*/

    // Cells from the 8-neighbourhood of the interior, are outer walls.
    let mut interior_neighbours: HashSet<(usize, usize)> = HashSet::new();

    for coordinates in &buildable_interior {
        for x in coordinates.0 - 1..=coordinates.0 + 1 {
            for z in coordinates.1 - 1..=coordinates.1 + 1 {
                if !buildable_interior.contains(&(x, z)) {
                    interior_neighbours.insert((x, z));
                }
            }
        }
    }

/*
    // FIXME remove: Mark actual wall.
    for (x, z) in &interior_neighbours {
        let y = height_map.height_at((*x, *z)).unwrap_or(255);
        let coordinates = BlockCoord(*x as i64, y as i64, *z as i64);
        output.set_block_at(coordinates, Block::Glass { colour: Some(mcprogedit::colour::Colour::Blue) });
    }
*/

    #[derive(Debug, Eq, Hash, PartialEq)]
    struct DoorPlacement {
        coordinates: (usize, usize),
        height: usize,
        facing: Surface4,
    }

    let mut possible_door_positions: HashSet<DoorPlacement> = HashSet::new();

    fn coordinates_in_direction(origo: &(usize, usize), direction: &Surface4, distance: usize) -> (usize, usize) {
        match direction {
            Surface4::North => (origo.0, origo.1 - distance),
            Surface4::South => (origo.0, origo.1 + distance),
            Surface4::East => (origo.0 + distance, origo.1),
            Surface4::West => (origo.0 - distance, origo.1),
        }
    }

    for (x, z) in &interior_neighbours {
        for direction in [Surface4::North, Surface4::South, Surface4::East, Surface4::West] {
            if buildable_interior.contains(&coordinates_in_direction(&(*x, *z), &direction, 1))
            && interior_neighbours.contains(&coordinates_in_direction(&(*x, *z), &direction.rotated_90_cw(), 1))
            && interior_neighbours.contains(&coordinates_in_direction(&(*x, *z), &direction.rotated_90_ccw(), 1)) {
                for distance in 1..=10 {
                    let look_at_coordinates = coordinates_in_direction(&(*x, *z), &direction.opposite(), distance);
                    match build_area.designation_at(look_at_coordinates) {
                        None => break,
                        Some(designation) => {
                            if designation.is_buildable() {
                                continue;
                            } else if designation.is_road() {
                                let height = height_map.height_at(look_at_coordinates).unwrap_or(255);
                                possible_door_positions.insert(DoorPlacement {
                                    coordinates: (*x, *z),
                                    height: height as usize,
                                    facing: direction,
                                });
                                // TODO break two loops here?
                                break;
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    /*
    // FIXME remove: Possible door positions.
    for door_position in &possible_door_positions {
        let (x, y, z) = (door_position.coordinates.0, door_position.height, door_position.coordinates.1);
        let coordinates = BlockCoord(x as i64, y as i64, z as i64);
        output.set_block_at(coordinates, Block::Glass { colour: Some(mcprogedit::colour::Colour::Gray) });
    }
    */

    // If there are no door positions, generation fails:
    if possible_door_positions.is_empty() {
        return None;
    }

    // Find highest and lowest possible door position.
    let highest_door_position = possible_door_positions.iter().max_by(|a, b| a.height.cmp(&b.height)).unwrap();
    let lowest_door_position = possible_door_positions.iter().max_by(|a, b| b.height.cmp(&a.height)).unwrap();

    /*
    // TODO remove: (one of the) highest possible door placements
    let (x, y, z) = (highest_door_position.coordinates.0, highest_door_position.height, highest_door_position.coordinates.1);
    let highest_door_position_coordinates = BlockCoord(x as i64, y as i64, z as i64);
    output.set_block_at(highest_door_position_coordinates, Block::BlockOfDiamond);
    let highest_door_position_coordinates = BlockCoord(x as i64, y as i64 + 1, z as i64);
    output.set_block_at(highest_door_position_coordinates, Block::BlockOfDiamond);
    */

    /*
    // TODO remove: (one of the) lowest possible door placements
    let (x, y, z) = (lowest_door_position.coordinates.0, lowest_door_position.height, lowest_door_position.coordinates.1);
    let lowest_door_position_coordinates = BlockCoord(x as i64, y as i64, z as i64);
    output.set_block_at(lowest_door_position_coordinates, Block::BlockOfGold);
    let lowest_door_position_coordinates = BlockCoord(x as i64, y as i64 + 1, z as i64);
    output.set_block_at(lowest_door_position_coordinates, Block::BlockOfGold);
    */

    let door_position_height_diff = highest_door_position.height - lowest_door_position.height;

    let door_positions = if door_position_height_diff == 0 {
        vec![lowest_door_position]
    } else if door_position_height_diff < 3 {
        // TODO Take some sort of median placement instead?
        vec![highest_door_position]
    } else {
        // TODO Check actual distance, try to put floors every 3 to 5 m.
        vec![lowest_door_position, highest_door_position]
    };

    // Find highest and lowest possible door position.
    let highest_door_position = door_positions.iter().max_by(|a, b| a.height.cmp(&b.height)).unwrap();
    let lowest_door_position = door_positions.iter().max_by(|a, b| b.height.cmp(&a.height)).unwrap();

    const STORY_HEIGHT: usize = 3;
    let cornice_height = highest_door_position.height + STORY_HEIGHT - 1;

    // Clear area from bottom floor to some distance above top floor.
    for (x, z) in &buildable_interior {
        for y in lowest_door_position.height..cornice_height {
            let coordinates = BlockCoord(*x as i64, y as i64, *z as i64);
            output.set_block_at(coordinates, Block::Air);
        }
    }

    // Place (base/cellar) walls from upper door down
    for (x, z) in &interior_neighbours {
        let lowest_y = min(lowest_door_position.height, height_map.height_at((*x, *z)).unwrap_or(255) as usize - 1);
        for y in lowest_y..=highest_door_position.height - 1 {
            let coordinates = BlockCoord(*x as i64, y as i64, *z as i64);
            output.set_block_at(coordinates, palette.foundation.clone());
        }
    }

    // Place walls from upper door up
    for (x, z) in &interior_neighbours {
        for y in highest_door_position.height..highest_door_position.height + STORY_HEIGHT {
            let coordinates = BlockCoord(*x as i64, y as i64, *z as i64);
            output.set_block_at(coordinates, palette.wall.clone());
        }
    }

    // Place doors.
    for door_position in &door_positions {
        let (x, y, z) = (door_position.coordinates.0, door_position.height, door_position.coordinates.1);
        let lower_coordinates = BlockCoord(x as i64, y as i64, z as i64);
        let upper_coordinates = BlockCoord(x as i64, y as i64 + 1, z as i64);
        output.set_block_at(lower_coordinates, Block::Door(mcprogedit::block::Door {
            material: mcprogedit::material::DoorMaterial::Oak,
            facing: door_position.facing,
            half: mcprogedit::block::DoorHalf::Lower,
            hinged_at: mcprogedit::block::Hinge::Right,
            open: false,
        }));
        output.set_block_at(upper_coordinates, Block::Door(mcprogedit::block::Door {
            material: mcprogedit::material::DoorMaterial::Oak,
            facing: door_position.facing,
            half: mcprogedit::block::DoorHalf::Upper,
            hinged_at: mcprogedit::block::Hinge::Right,
            open: false,
        }));
    }

    // Decide floor levels.
    let mut floor_levels: HashSet<i64> = HashSet::new();
    for door_position in &door_positions {
        floor_levels.insert(door_position.height as i64 - 1);
    }

    // Place floors.
    for y in &floor_levels {
        for (x, z) in &buildable_interior {
            let coordinates = BlockCoord(*x as i64, *y as i64, *z as i64);
            output.set_block_at(coordinates, palette.floor.clone());
        }
    }

    // Find possible window locations
    let mut possible_window_coordinates: HashSet<BlockCoord> = HashSet::new();
    for y in &floor_levels {
        'wall_piece: for (x, z) in &interior_neighbours {
            for direction in [Surface4::North, Surface4::South, Surface4::East, Surface4::West] {
                let inside = coordinates_in_direction(&(*x, *z), &direction, 1);
                let first_side = coordinates_in_direction(&(*x, *z), &direction.rotated_90_cw(), 1);
                let second_side = coordinates_in_direction(&(*x, *z), &direction.rotated_90_ccw(), 1);
                let outside = coordinates_in_direction(&(*x, *z), &direction.opposite(), 1);

                if buildable_interior.contains(&inside)
                && interior_neighbours.contains(&first_side)
                && interior_neighbours.contains(&second_side) {
                    // Check if door (or next to door)
                    for door_position in &door_positions {
                        if door_position.height == *y as usize + 1
                        && [(*x, *z), first_side, second_side].contains(&door_position.coordinates) {
                            // Window would collide with a door.
                            continue 'wall_piece;
                        }
                    }

                    let outside_coordinates = coordinates_in_direction(&(*x, *z), &direction.opposite(), 1);

                    // Check if under ground
                    if let Some(outside_height) = height_map.height_at((*x, *z)) {
                        if outside_height as i64 > y + 2 {
                            // Coordinates are below ground level.
                            continue 'wall_piece;
                        }
                    }

                    // Check if the outside area is actually open air.
                    if let Some(outside_designation) = build_area.designation_at(outside_coordinates) {
                        if outside_designation.is_buildable() || outside_designation.is_road() {
                            // This looks like a perfectly fine place to put a window.
                            possible_window_coordinates.insert(BlockCoord(*x as i64, y + 2, *z as i64));
                            continue 'wall_piece;
                        }
                    }
                }
            }
        }
    }

    // Find rows of windows, and split them up a bit.
    let mut window_splits: HashSet<BlockCoord> = HashSet::new();
    for possible_window_coordinate in &possible_window_coordinates {
        for direction in [BlockCoord(1, 0, 0), BlockCoord(0, 0, 1)] {
            if possible_window_coordinates.contains(&(*possible_window_coordinate - direction)) {
                // Not end of row, nothing to check.
                continue;
            }

            // Count windows in direction.
            let mut count = 0;
            let mut coordinate = *possible_window_coordinate;
            while possible_window_coordinates.contains(&coordinate) {
                count += 1;
                coordinate = coordinate + direction;
            }

            // Register splits for long rows.
            let removal_remainder = match count % 3 {
                0 => 1,
                _ => 2,
            };

            // Add every ''3n + removal_remainder'' to window_splits
            let mut coordinate = *possible_window_coordinate;
            for index in 0..count {
                if index % 3 == removal_remainder {
                    window_splits.insert(coordinate);
                }
                coordinate = coordinate + direction;
            }
        }
    }
    for split_coordinates in &window_splits {
        possible_window_coordinates.remove(split_coordinates);
    }

    // Place windows
    for window_coordinates in &possible_window_coordinates {
        output.set_block_at(*window_coordinates, Block::Glass { colour: None });
    }

    // Calculate and place roof
    let roof_coordinates = calculate_roof_coordinates(&interior_neighbours, &buildable_interior, cornice_height);
    for coordinates in roof_coordinates {
        output.set_block_at(coordinates, palette.roof.clone());

        // If over internal parts: Clear down to cornice_height
        if buildable_interior.contains(&(coordinates.0 as usize, coordinates.2 as usize)) {
            for air_y in cornice_height as i64..coordinates.1 {
                let air_coordinates = BlockCoord(coordinates.0, air_y, coordinates.2);
                output.set_block_at(air_coordinates, Block::Air);
            }
        }

        // If over wall; Wall down to cornice_height
        if interior_neighbours.contains(&(coordinates.0 as usize, coordinates.2 as usize)) {
            for wall_y in cornice_height as i64..coordinates.1 {
                let wall_coordinates = BlockCoord(coordinates.0, wall_y, coordinates.2);
                output.set_block_at(wall_coordinates, palette.wall.clone());
            }
        }
    }

    // Place some flowers in suitable areas around the house.
    // TODO For all buildable area around building.
    // TODO With some probability.
    // TODO If fertile ground: Plant flowers.
    // TODO If non-fertile ground: Put down flower pots.

    let outside_area: HashSet<(usize, usize)> = road_along_buildable
        .union(&buildable).cloned().collect::<HashSet<(usize, usize)>>()
        .difference(&buildable_interior).cloned().collect::<HashSet<(usize, usize)>>()
        .difference(&interior_neighbours).cloned().collect::<HashSet<(usize, usize)>>();

    if !palette.flowers.is_empty() {
        for (index, (x, z)) in outside_area.iter().enumerate() {
            // Only attempt flower placement once in a while
            if index % 3 != 0 {
                continue;
            }

            if let Some(y) = height_map.height_at((*x, *z)) {
                let ground_coordinates = BlockCoord(*x as i64, y as i64 - 1, *z as i64);
                let bottom_coordinates = BlockCoord(*x as i64, y as i64, *z as i64);
                let top_coordinates = BlockCoord(*x as i64, y as i64 + 1, *z as i64);
                match excerpt.block_at(ground_coordinates) {
                    Some(Block::GrassBlock)
                    | Some(Block::CoarseDirt)
                    | Some(Block::Dirt)
                    | Some(Block::Podzol) => {
                        // Decide on flower type
                        let flower_index = index % min(8, palette.flowers.len());

                        // Bottom part
                        output.set_block_at(bottom_coordinates, Block::Flower(palette.flowers[flower_index]));

                        // Top part
                        match palette.flowers[flower_index] {
                            Flower::LilacBottom => {
                                output.set_block_at(top_coordinates, Block::Flower(Flower::LilacTop));
                            }
                            Flower::PeonyBottom => {
                                output.set_block_at(top_coordinates, Block::Flower(Flower::PeonyTop));
                            }
                            Flower::RoseBushBottom => {
                                output.set_block_at(top_coordinates, Block::Flower(Flower::RoseBushTop));
                            }
                            Flower::SunflowerBottom => {
                                output.set_block_at(top_coordinates, Block::Flower(Flower::SunflowerTop));
                            }
                            _ => (),
                        }
                    }
                    Some(Block::Sand)
                    | Some(Block::Sandstone)
                    | Some(Block::RedSand)
                    | Some(Block::RedSandstone)
                    | Some(Block::Stone) => {
                        // Decide on flower type
                        let flower_index = index % min(8, palette.flowers.len());

                        let flower_pot: mcprogedit::block::FlowerPot = palette.flowers[flower_index].into();
                        output.set_block_at(
                            bottom_coordinates,
                            Block::FlowerPot(flower_pot),
                        );
                    }
                    _ => (),
                }
            }
        }
    }

    /*
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
    }*/

    Some(output)
}

fn calculate_roof_coordinates(
    outline: &HashSet<(usize, usize)>,
    interior: &HashSet<(usize, usize)>,
    height: usize,
) -> HashSet<BlockCoord> {
    let mut roof: HashSet<BlockCoord> = HashSet::new();

    let split_lines = compute_split_lines(outline);

    // TODO: Actually use this for something, e.g. deciding type of roof.
    // Gather some stats on the split lines (only the lengths, for now)
    let (short_split_line, long_split_line) = split_lines;
    let short_len = geometry::manhattan_distance(short_split_line.0, short_split_line.1);
    let long_len = geometry::manhattan_distance(long_split_line.0, long_split_line.1);
    trace!("Roof split lines are of length {} and {}.", short_len, long_len);

    // Calculate a gable roof
    let gable_height = height + (short_len / 2);
    let gable_line = (
        BlockCoord(long_split_line.0.0, gable_height as i64, long_split_line.0.1),
        BlockCoord(long_split_line.1.0, gable_height as i64, long_split_line.1.1),
    );
    let mut to_place: HashSet<BlockCoord> = line(&gable_line.0, &gable_line.1, 1).into_iter().collect();

    if to_place.is_empty() {
        warn!("No blocks in roof gable.");
        return roof;
    }

    let mut unplaced: HashSet<(usize, usize)> = outline.union(interior).copied().collect();
    let mut already_handled: HashSet<(usize, usize)> = HashSet::new();

    while !unplaced.is_empty() {
        // Handle coordinates to be placed in this iteration
        for coordinates in &to_place {
            let coordinates_2d = (coordinates.0 as usize, coordinates.2 as usize);

            already_handled.insert(coordinates_2d);

            if unplaced.contains(&coordinates_2d) {
                roof.insert(*coordinates);
                unplaced.remove(&coordinates_2d);
            }
        }

        // Find coordinates for next iteration
        let mut neighbourhood: HashSet<BlockCoord> = to_place.iter().map(|coordinates| [
                                                     BlockCoord(coordinates.0 + 1, coordinates.1 - 1, coordinates.2),
                                                     BlockCoord(coordinates.0 - 1, coordinates.1 - 1, coordinates.2),
                                                     BlockCoord(coordinates.0, coordinates.1 - 1, coordinates.2 + 1),
                                                     BlockCoord(coordinates.0, coordinates.1 - 1, coordinates.2 - 1),
        ]).flatten().collect();
        neighbourhood.retain(|coordinates| !already_handled.contains(&(coordinates.0 as usize, coordinates.2 as usize)));
        to_place = neighbourhood;
    }

    // Adjust roof y positioning
    let lowest_y = roof.iter().max_by(|a, b| b.1.cmp(&a.1)).unwrap().1;
    if lowest_y != height as i64 {
        trace!("Roof is offset by {}!", lowest_y - height as i64);
        let offset = BlockCoord(0, lowest_y - height as i64, 0);
        let mut adjusted_roof = HashSet::new();
        for coordinates in roof {
            adjusted_roof.insert(coordinates - offset);
        }
        roof = adjusted_roof;
    } else {
        trace!("Roof is at perfect height!");
    }

    roof
}

fn compute_split_lines(points: &HashSet<(usize, usize)>) -> (RawEdge2d, RawEdge2d) {
    let point_vec: Vec<imageproc::point::Point<i64>> = points
        .iter()
        .map(|point| imageproc::point::Point::<i64>::new(point.0 as i64, point.1 as i64))
        .collect();
    let obb = imageproc::geometry::min_area_rect(&point_vec);

    let (p0, p1, p2, p3) = (obb[0], obb[1], obb[2], obb[3]);

    let split_line_0 = (
        (BlockColumnCoord(p0.x, p0.y) + BlockColumnCoord(p1.x, p1.y)) / 2,
        (BlockColumnCoord(p2.x, p2.y) + BlockColumnCoord(p3.x, p3.y)) / 2,
    );
    let split_line_1 = (
        (BlockColumnCoord(p1.x, p1.y) + BlockColumnCoord(p2.x, p2.y)) / 2,
        (BlockColumnCoord(p3.x, p3.y) + BlockColumnCoord(p0.x, p0.y)) / 2,
    );

    // Figure out which one is the short one and which one is the long one.
    let len_0 = geometry::euclidean_distance(split_line_0.0, split_line_0.1);
    let len_1 = geometry::euclidean_distance(split_line_1.0, split_line_1.1);

    // Return the short one first
    if len_0 < len_1 {
        (split_line_0, split_line_1)
    } else {
        (split_line_1, split_line_0)
    }
}

pub fn build_legacy_house(
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
    let mut height_map = excerpt.ground_height_map();

    // "Clean up" the build area a bit, by removing weird outliers.
    let mut changes = 1;
    while changes > 0 {
        changes = 0;
        let mut to_remove = Vec::new();

        for coordinates in &buildable_edge {
            let mut outside_neighbours_count = 0;
            let mut road_accessible_neighbours_count = 0;
            for x in coordinates.0 - 1..=coordinates.0 + 1 {
                for z in coordinates.1 - 1..=coordinates.1 + 1 {
                    if not_buildable.contains(&(x, z)) {
                        outside_neighbours_count += 1;
                    }
                    if road_along_buildable.contains(&(x, z)) {
                        road_accessible_neighbours_count += 1;
                    }
                }
            }
            if outside_neighbours_count > 5 {
                changes += 1;
                buildable.remove(coordinates);
                to_remove.push(*coordinates);
                not_buildable.insert(*coordinates);
                if road_accessible_neighbours_count > 0 {
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
