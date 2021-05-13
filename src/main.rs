//! Leifsbudir - settlement generator for Minecraft

extern crate clap;
extern crate mcprogedit;

mod areas;
mod features;
mod line;
mod pathfinding;
mod types;
mod wall;
mod walled_town;

use std::path::Path;

use mcprogedit::block::Block;
use mcprogedit::coordinates::{BlockColumnCoord, BlockCoord};
use mcprogedit::material::Material;
use mcprogedit::positioning::Axis3;
use mcprogedit::world_excerpt::WorldExcerpt;

use crate::areas::*;
use crate::features::*;
use crate::walled_town::*;

fn main() {
    // Read arguments
    // **************
    let matches = matches();
    let input_directory = matches.value_of("input_save").unwrap_or(".");
    let output_directory = matches.value_of("output_save").unwrap_or(input_directory);
    let x = matches.value_of("x").map(parse_i64_or_exit).unwrap();
    let y = matches.value_of("y").map(parse_i64_or_exit).unwrap_or(0);
    let z = matches.value_of("z").map(parse_i64_or_exit).unwrap();
    let x_len = matches.value_of("dx").map(parse_i64_or_exit).unwrap();
    let y_len = matches
        .value_of("dy")
        .map(parse_i64_or_exit)
        .unwrap_or(255 - y);
    let z_len = matches.value_of("dz").map(parse_i64_or_exit).unwrap();


    // World import
    // ************
    println!("Importing from {:?}", input_directory);
    let mut excerpt = WorldExcerpt::from_save(
        (x, y, z).into(),
        (x + x_len - 1, y + y_len - 1, z + z_len - 1).into(),
        Path::new(input_directory),
    );
    println!("Imported world excerpt of dimensions {:?}", excerpt.dim());


    // Initial information extraction
    // ******************************
    let _player_location: BlockColumnCoord = (x_len / 2, z_len / 2).into();

    // Extract features
    let features = Features::new_from_world_excerpt(&excerpt);

    // Find areas suitable for various purposes (based on features)
    let areas = Areas::new_from_features(&features);


    // Decide on area usage
    // ********************
    // Some thoughts:
    // - Fields on fertile, reasonably flat, open land
    // - Wind mills on hills within or by fertile land
    // - Fields closer to wind mills are predominantly wheat fields
    // - Livestock on fertile, flat to half-steep, open to semi-open land
    // - Forestry on forested land
    // - Mining on exposed rock, either surface (quarry) or hillside (mining tunnel)
    // - Fishing on shorelines with access to sea
    // - Infrastructure: Maybe connect "traversable" areas through bridges, tunnels, etc?
    // - Town is complicated. Can to some extent displace fields/livestock

    // Find town location
    let (town_circumference, town_center) = walled_town_contour(&features, &areas);

    // Create some paths... (NB Only for generating a cool image. Not built in world.)
    let start = (x_len as usize / 2, z_len as usize / 2);
    let mut path_image = features.coloured_map.clone();

    for goal in &town_circumference {
        if let Some(path) = pathfinding::path(start, *goal, &features.terrain) {
            draw_snake(&mut path_image, &path);
        }
    }
    path_image.save("path_001.png").unwrap();

    // Build wall
    let mut wall_circle = town_circumference.clone();
    wall_circle.push(town_circumference[0]);
    wall::build_wall(&mut excerpt, &wall_circle, &features);

    // Create road paths...
    let start_coordinates: Vec<_> = vec![
        (0, 0),
        (0, z_len - 1),
        (x_len - 1, z_len - 1),
        (x_len - 1, 0),
    ]
    .iter()
    .map(|coordinates| {
        let (x, z) = *coordinates;
        let image::Luma([y]) = features.terrain[(x as u32, z as u32)];
        BlockCoord(x, y as i64, z)
    })
    .collect();

    let image::Luma([goal_y]) = features.terrain[
        (town_center.0 as u32, town_center.1 as u32)
    ];
    let goal = BlockCoord(town_center.0 as i64, goal_y as i64, town_center.1 as i64);

    let mut road_path_image = features.coloured_map.clone();

    for start in start_coordinates {
        if let Some(path) = pathfinding::road_path(
            start,
            goal,
            &features.terrain,
            &imageproc::morphology::dilate(
                &features.water,
                imageproc::distance_transform::Norm::LInf,
                2,
            ),
        ) {
            // Draw road on map
            pathfinding::draw_road_path(&mut road_path_image, &path);

            // Build the nodes
            for pathfinding::RoadNode { coordinates, kind, .. } in &path {
                let (x, y, z) = (coordinates.0, coordinates.1, coordinates.2);

                // Space above the nodes
                excerpt.set_block_at((x, y, z).into(), Block::Air);
                excerpt.set_block_at((x, y+1, z).into(), Block::Air);
                excerpt.set_block_at((x, y+2, z).into(), Block::Air);

                // Path and support at node
                match kind {
                    pathfinding::RoadNodeKind::Ground => {
                        excerpt.set_block_at(
                            (x, y-1, z).into(),
                            Block::double_slab(Material::SmoothStone)
                        );
                    }
                    pathfinding::RoadNodeKind::WoodenSupport => {
                        let ground = features.terrain_height_map.height_at((x as usize, z as usize))
                                .unwrap_or(0) as i64;
                        for y in ground..y {
                            excerpt.set_block_at((x, y, z).into(), Block::oak_log(Axis3::Y));
                        }
                    }
                    pathfinding::RoadNodeKind::StoneSupport => {
                        let ground = features.terrain_height_map.height_at((x as usize, z as usize))
                                .unwrap_or(0) as i64;
                        for y in ground..y {
                            excerpt.set_block_at((x, y, z).into(), Block::StoneBricks);
                        }
                    }
                    _ => (),
                }
            }

            // Build the path segments
            for segment in path.windows(2) {
                let line = line::line(
                    &(segment[0].coordinates),
                    &(segment[1].coordinates),
                    3,
                );
                for position in line {
                    excerpt.set_block_at(position - (0, 2, 0).into(), Block::Cobblestone);
                    excerpt.set_block_at(position - (0, 1, 0).into(), Block::Gravel);
                    excerpt.set_block_at(position, Block::Air);
                    excerpt.set_block_at(position + (0, 1, 0).into(), Block::Air);
                    excerpt.set_block_at(position + (0, 2, 0).into(), Block::Air);
                }
            }
        }
    }

    road_path_image.save("road_path_001.png").unwrap();

    // TODO
    // - Find primary sector areas (agriculture, fishing, forestry, mining)
    // - Find suitable town circumference (may depend on primary sector areas)
    // - Put major roads from primary sectors to town circumference
    // - Extend and connect major roads inside town
    // - Fill out with minor roads inside town
    // - Fill out with plots inside town
    // - If player location is inside town, not on road, then make square plot there
    // - If player location is outside town, make road from there to nearest major road,
    //   and put signs towards town. Bridges, boat trips, etc. may be needed...
    // - Build structures on plots


    // World export
    // ************
    println!("Exporting to {:?}", output_directory);
    excerpt.to_save((x, y, z).into(), Path::new(output_directory));
    println!("Exported world excerpt of dimensions {:?}", excerpt.dim());
}

fn parse_i64_or_exit(string: &str) -> i64 {
    string.parse::<i64>().unwrap_or_else(|_| {
        eprintln!("Not an integer: {}", string);
        std::process::exit(1);
    })
}

fn matches() -> clap::ArgMatches<'static> {
    clap::App::new("casg - Cellular Automata Settlement Generator.")
        .set_term_width(80)
        .version(clap::crate_version!())
        .arg(
            clap::Arg::with_name("input_save")
                .short("-i")
                .long("input-directory")
                .value_name("DIRECTORY")
                .help("Input save directory. Set to working directory if not provided.")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("output_save")
                .short("-o")
                .long("output-directory")
                .value_name("DIRECTORY")
                .help("Output save directory. Set to input directory if not provided.")
                .takes_value(true),
        )
        .arg(
            clap::Arg::with_name("x")
                .short("-x")
                .long("x-coordinate")
                .value_name("block x")
                .help("Selection corner x coordinate.")
                .takes_value(true)
                .number_of_values(1)
                .allow_hyphen_values(true)
                .required(true),
        )
        .arg(
            clap::Arg::with_name("dx")
                .short("-X")
                .long("x-size")
                .value_name("block count")
                .help("Selection size along the x axis.")
                .takes_value(true)
                .number_of_values(1)
                .allow_hyphen_values(true)
                .required(true),
        )
        .arg(
            clap::Arg::with_name("y")
                .short("-y")
                .long("y-coordinate")
                .value_name("block y")
                .help("Selection corner y coordinate.")
                .takes_value(true)
                .number_of_values(1)
                .allow_hyphen_values(true)
                .required(false),
        )
        .arg(
            clap::Arg::with_name("dy")
                .short("-Y")
                .long("y-size")
                .value_name("block count")
                .help("Selection size along the y axis.")
                .takes_value(true)
                .number_of_values(1)
                .allow_hyphen_values(true)
                .required(false),
        )
        .arg(
            clap::Arg::with_name("z")
                .short("-z")
                .long("z-coordinate")
                .value_name("block z")
                .help("Selection corner z coordinate.")
                .takes_value(true)
                .number_of_values(1)
                .allow_hyphen_values(true)
                .required(true),
        )
        .arg(
            clap::Arg::with_name("dz")
                .short("-Z")
                .long("z-size")
                .value_name("block count")
                .help("Selection size along the z axis.")
                .takes_value(true)
                .number_of_values(1)
                .allow_hyphen_values(true)
                .required(true),
        )
        .get_matches()
}
