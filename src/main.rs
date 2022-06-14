//! Leifsbudir - settlement generator for Minecraft

extern crate clap;
extern crate mcprogedit;

mod areas;
mod block_palette;
mod build_area;
mod features;
mod geometry;
mod line;
mod partitioning;
mod pathfinding;
mod plot;
mod road;
mod room_interior;
mod structure_builder;
mod tree;
mod types;
mod wall;
mod walled_town;

use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use log::{error, info, LevelFilter};
use simple_logger::SimpleLogger;

use imageproc::stats::histogram;
use mcprogedit::block::{Block, Log};
use mcprogedit::coordinates::{BlockColumnCoord, BlockCoord};
use mcprogedit::material::{CoralMaterial, WoodMaterial};
use mcprogedit::world_excerpt::WorldExcerpt;

use crate::areas::*;
use crate::block_palette::BlockPalette;
use crate::features::*;
use crate::geometry::{extract_blocks, LandUsageGraph};
use crate::partitioning::divide_town_into_blocks;
use crate::plot::divide_city_block;
use crate::road::roads_split;
use crate::walled_town::*;

fn main() {
    // Initialize logging
    SimpleLogger::new().with_level(LevelFilter::Warn).init().unwrap();

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
    info!("Importing from {:?}", input_directory);
    let mut excerpt = WorldExcerpt::from_save(
        (x, y, z).into(),
        (x + x_len - 1, y + y_len - 1, z + z_len - 1).into(),
        Path::new(input_directory),
    );
    info!("Imported world excerpt of dimensions {:?}", excerpt.dim());


    // Initial information extraction
    // ******************************
    let player_location: BlockColumnCoord = (x_len / 2, z_len / 2).into();

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
    // - Town is complicated. Can to some extent displace fields/livestock/forest

    // Find town location
    let (town_circumference, town_center) = walled_town_contour(&features, &areas);

    // Get full wall circle, by copying the first node of the wall to the end.
    let mut wall_circle = town_circumference.clone();
    wall_circle.push(town_circumference[0]);

    // Get town size
    let town_area = geometry::area(&wall_circle);
    info!("The found city has a total area of {} m².", town_area);

    // TODO FUTURE WORK
    // - Find primary sector areas (agriculture, fishing, forestry, mining)
    // - Put major roads from primary sectors to town circumference
    // - Actually, find more settlement locations as well,
    //      and use some nice triangulation for connecting everything.
    //      (e.g. Delaunay, Gabriel graph, or Relative neighbourhood graph.)

    // Create road paths...
    // TODO refactor: Move the path generation somewhere else?
    // TODO to be replaced by other means of finding road start locations
    let mut start_coordinates = vec![
        // Paths from the four corners of the map
        (0, 0),
        (0, z_len - 1),
        (x_len - 1, z_len - 1),
        (x_len - 1, 0),
    ];

    if geometry::InOutSide::Outside == geometry::point_position_relative_to_polygon(player_location.clone(), &wall_circle) {
        // Path from the player start location
        start_coordinates.push((player_location.0, player_location.1));
    }

    let start_coordinates: Vec<_> = start_coordinates
    .iter()
    .map(|(x, z)| {
        let image::Luma([y]) = features.terrain[(*x as u32, *z as u32)];
        BlockCoord(*x, y as i64, *z)
    })
    .collect();

    let image::Luma([goal_y]) = features.terrain[
        (town_center.0 as u32, town_center.1 as u32)
    ];
    let goal = BlockCoord(town_center.0 as i64, goal_y as i64, town_center.1 as i64);

    let mut road_path_image = features.coloured_map.clone();

    let mut raw_roads = Vec::new();

    for start in start_coordinates {
        if let Some(path) = pathfinding::road_path(
            start,
            goal,
            &features.terrain,
            Some(
                &imageproc::morphology::dilate(
                    &features.water,
                    imageproc::distance_transform::Norm::LInf,
                    2,
                )
            ),
        ) {
            // Draw road on map
            pathfinding::draw_road_path(&mut road_path_image, &path);

            // Store road
            raw_roads.push(path);
        }
    }

    #[cfg(feature = "debug_images")]
    road_path_image.save("road_path_001.png").unwrap();

    // Split out the raw roads into city roads and country roads
    let (mut city_roads, country_roads) = roads_split(&raw_roads, &wall_circle);

    // Fill out with minor roads inside town
    let mut streets =
        divide_town_into_blocks(&town_circumference, &town_center, &city_roads, &features.terrain);


    // Make land usage plan
    // ********************

    // Add intersection points between roads/streets and circumference,
    // so that the geometry actually describes distinct areas.
    geometry::add_intersection_points(&mut streets, &mut wall_circle);
    geometry::add_intersection_points(&mut city_roads, &mut wall_circle);

    // TODO decide width of streets/roads/walls based on total town area?
    let mut land_usage_graph = LandUsageGraph::new();
    land_usage_graph.add_roads(&streets, geometry::EdgeKind::Street, 2);
    land_usage_graph.add_roads(&city_roads, geometry::EdgeKind::Road, 6);
    land_usage_graph.add_circumference(&wall_circle, geometry::EdgeKind::Wall, 3);

    // Get the polygons for each "city block"
    let districts = extract_blocks(&land_usage_graph);

    // Make images of the extracted city blocks (for debug visuals only)
    for (colour, district) in districts.iter().enumerate() {
        let mut district_image = image::ImageBuffer::new(x_len as u32, z_len as u32);
        geometry::draw_area(
            &mut district_image,
            district,
            BlockColumnCoord(0, 0),
            image::Luma([63u8]),
        );
        partitioning::draw_offset_snake(
            &mut district_image,
            district,
            &BlockColumnCoord(0, 0),
            image::Luma([255u8]),
        );

        #[cfg(feature = "debug_images")]
        district_image.save(format!("D-01 district {:0>2}.png", colour)).unwrap();

        info!("District {} has area {}.", colour, geometry::area(district));
    
        let stats = histogram(&district_image);
        let surface_area = stats.channels[0][63];
        let border_area = stats.channels[0][255];
        info!(
            "District {} image areas: {} + ({} / 2) = {}",
            colour, surface_area, border_area, surface_area + (border_area / 2)
        );
    }

    // TODO Save only if debug images is enabled
    //district_image.save("D-01 districts.png").unwrap();

    // Split the city blocks
    let mut plots = Vec::new();
    for district in districts {
        let mut district_plots = divide_city_block(&district, &land_usage_graph);
        // TODO draw the plots or something...
        info!("Found {} plots for a district.", district_plots.len());
        plots.append(&mut district_plots);
    }

    let mut city_plan = features.coloured_map.clone();
    for plot in &plots {
        plot.draw(&mut city_plan);
    }
    for street in &streets {
        pathfinding::draw_road_path(&mut city_plan, street);
    }
    for road in &country_roads {
        pathfinding::draw_road_path(&mut city_plan, road);
    }
    for road in &city_roads {
        pathfinding::draw_road_path(&mut city_plan, road);
    }

    #[cfg(feature = "debug_images")]
    city_plan.save("city plan.png").unwrap();


    // Find local materials
    // ********************

    // Survey the area inside and around town, to find local materials.
    let (town_offset, town_dimensions) = partitioning::snake_bounding_box(&wall_circle);

    let proximity_min_x = town_offset.0.saturating_sub(100);
    let proximity_max_x = min(x_len, town_offset.0 + town_dimensions.0 + 100);
    let proximity_min_z = town_offset.1.saturating_sub(100);
    let proximity_max_z = min(z_len, town_offset.1 + town_dimensions.0 + 100);

    let mut sand_count = 0;
    let mut grass_count = 0;
    let mut available_flowers = HashSet::new();
    let mut wood_statistics = HashMap::new();

    for x in proximity_min_x..proximity_max_x {
        for z in proximity_min_z..proximity_max_z {
            if let Some(terrain_y) = features.terrain_height_map.height_at(
                (x as usize, z as usize)
            ) {
                for y in terrain_y-1..terrain_y+1 {
                    match excerpt.block_at(BlockCoord(x, y as i64, z)) {
                        // Make some statistics
                        Some(Block::Sand) => sand_count += 1,
                        Some(Block::GrassBlock) => grass_count += 1,
                        Some(Block::Flower(flower)) => {
                            available_flowers.insert(*flower);
                        }
                        Some(Block::Log(Log { material, .. })) => {
                            *wood_statistics.entry(*material).or_insert(0) += 1;
                        }
                        _ => (),
                    }
                }
            }
        }
    }

    let mut wood_statistics: Vec<_> = wood_statistics.into_iter().collect();
    wood_statistics.sort_by(|a, b| a.1.cmp(&b.1).reverse());

    // wood_available to be used later, for replacing wall/roof materials in the
    // block palette used for building individual houses.
    let mut wood_available = Vec::new();
    let max_wood_count = if let Some((_, count)) = wood_statistics.first() {
        *count
    } else {
        0
    };
    for (wood, count) in wood_statistics {
        if count >= max_wood_count / 50 {
            wood_available.push(wood);
        }
    }
    // Sort the woods by colour in order not to get too psychedelic.
    wood_available.sort_by_key(|wood_material| match wood_material {
        WoodMaterial::Acacia => 5,
        WoodMaterial::Birch => 4,
        WoodMaterial::DarkOak => 0,
        WoodMaterial::Jungle => 3,
        WoodMaterial::Oak => 2,
        WoodMaterial::Spruce => 1,
        _ => 6,
    });

    info!("Decided that {:?} are the common wood materials.", wood_available);

    // Use found materials for a default block palette
    let mut block_palette = BlockPalette {
        flowers: available_flowers.clone().into_iter().collect(),
        ..Default::default()
    };

    if sand_count > grass_count {
        // Assume that we are in or close to a desert biome;
        // Use sandstone instead of stone, for city wall and other "stone" structures.
        block_palette.city_wall_coronation = Block::Sandstone;
        block_palette.city_wall_main = Block::Sandstone;
        block_palette.city_wall_top = Block::SmoothSandstone;
        block_palette.foundation = Block::EndStoneBricks;
        block_palette.floor = Block::SmoothSandstone;
        block_palette.wall = Block::Sandstone;
    }

    info!(
        "Found {} different flowers.",
        available_flowers.len(),
    );


    // Build structures
    // ****************

    // Build that wall! (But who is going to pay for it?)
    wall::build_wall(&mut excerpt, &wall_circle, &features, &block_palette);

    // Build the various roads and streets...
    // TODO Change road width depending on total town area?
    let city_streets_cover = vec![
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::CoralBlock { material: CoralMaterial::Fire, dead: true },
        Block::CoralBlock { material: CoralMaterial::Fire, dead: true },
        Block::CoralBlock { material: CoralMaterial::Horn, dead: true },
    ];
    for street in streets {
        road::build_road(&mut excerpt, &street, &features.terrain, 2, &city_streets_cover);
    }

    let country_roads_cover = vec![
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::CoralBlock { material: CoralMaterial::Fire, dead: true },
        Block::CoralBlock { material: CoralMaterial::Fire, dead: true },
        Block::CoralBlock { material: CoralMaterial::Fire, dead: true },
        Block::CoralBlock { material: CoralMaterial::Fire, dead: true },
        Block::CoralBlock { material: CoralMaterial::Bubble, dead: true },
        Block::CoralBlock { material: CoralMaterial::Horn, dead: true },
        Block::CoralBlock { material: CoralMaterial::Tube, dead: true },
        Block::CoarseDirt,
        Block::CoarseDirt,
        Block::CoarseDirt,
    ];
    for road in country_roads {
        road::build_road(&mut excerpt, &road, &features.terrain, 3, &country_roads_cover);
    }

    let city_roads_cover = vec![
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::Gravel,
        Block::CoralBlock { material: CoralMaterial::Fire, dead: true },
        Block::CoralBlock { material: CoralMaterial::Fire, dead: true },
        Block::CoralBlock { material: CoralMaterial::Fire, dead: true },
        Block::CoralBlock { material: CoralMaterial::Fire, dead: true },
        Block::Andesite,
        Block::Andesite,
        Block::CoralBlock { material: CoralMaterial::Bubble, dead: true },
        Block::CoralBlock { material: CoralMaterial::Horn, dead: true },
        Block::CoralBlock { material: CoralMaterial::Tube, dead: true },
        Block::CrackedStoneBricks,
        Block::CrackedStoneBricks,
        Block::StoneBricks,
        Block::Cobblestone,
        Block::Cobblestone,
    ];
    for road in city_roads {
        road::build_road(&mut excerpt, &road, &features.terrain, 4, &city_roads_cover);
    }

    // Build some structures (houses?) on the plots.
    for (index, plot) in plots.iter().enumerate() {
        // Skip every Nth plot
        if index % 10 == 9 {
            continue;
        }

        if let Some(bounding_box) = plot.bounding_box() {
            // Increase the size by 1, in order to provide at least one block of context.
            let mut bounding_box = (
                bounding_box.0 - BlockCoord(1, 0, 1),
                bounding_box.1 + BlockCoord(1, 0, 1),
            );
            bounding_box.0 .1 = 0;
            bounding_box.1 .1 = y_len - 1;

            // Get the relative plot description and relative world excerpt
            let offset_plot = plot.offset(bounding_box.0);
            let plot_excerpt = WorldExcerpt::from_world_excerpt(
                (bounding_box.0 .0 as usize, bounding_box.0 .1 as usize, bounding_box.0 .2 as usize),
                (bounding_box.1 .0 as usize, bounding_box.1 .1 as usize, bounding_box.1 .2 as usize),
                &excerpt,
            );

            // Get the build area description structure for the (now offset) plot
            let plot_build_area =
                build_area::BuildArea::from_world_excerpt_and_plot(&plot_excerpt, &offset_plot);

            // Modify the palette, depending on the diversity of available wood
            let mut custom_palette = block_palette.clone();
            if wood_available.is_empty() {
                // Sadly no wood to use here.
                // Replace some roofs with other materials
                match index % 7 {
                    0 | 2 | 4 => custom_palette.roof = custom_palette.floor.clone(),
                    _ => (),
                }
            } else if wood_available.len() == 1 {
                // Replace most walls with the available wood
                match index % 4 {
                    0 | 1 | 2 => {
                        custom_palette.foundation = block_palette.wall.clone();
                        custom_palette.wall = Block::Planks { material: wood_available[0] };
                    }
                    // If the walls were not replaced, replace the floor instead.
                    _ => {
                        custom_palette.floor = Block::Planks { material: wood_available[0] };
                    },
                }
                // Replace some roofs with other materials
                match index % 7 {
                    0 | 2 | 4 => custom_palette.roof = custom_palette.floor.clone(),
                    _ => (),
                }
            } else if wood_available.len() == 2 {
                // Replace all roofs with one kind of wood.
                custom_palette.roof = Block::Planks { material: wood_available[0] };
                // Replace most walls with the other kind of wood.
                match index % 4 {
                    0 | 1 | 2 => {
                        custom_palette.foundation = block_palette.wall.clone();
                        custom_palette.wall = Block::Planks { material: wood_available[1] };
                    }
                    // If the walls were not replaced, replace the floor instead.
                    _ => {
                        custom_palette.floor = Block::Planks { material: wood_available[1] };
                    },
                }
                // Replace some roofs with other materials
                match index % 7 {
                    0 | 2 | 4 => custom_palette.roof = custom_palette.floor.clone(),
                    _ => (),
                }
            } else {
                // Replace all roofs with one kind of wood.
                custom_palette.roof = Block::Planks { material: wood_available[1] };
                // Replace most walls with one of the other kinds of wood.
                match index % 4 {
                    0 | 1 | 2 => {
                        custom_palette.foundation = block_palette.wall.clone();
                        custom_palette.wall = Block::Planks { material: wood_available[2] };
                    }
                    _ => (),
                }
                // Replace quite a few floors with the other remaining kind of wood.
                match index % 5 {
                    0 | 1 | 2 => {
                        custom_palette.floor = Block::Planks { material: wood_available[0] };
                    }
                    _ => (),
                }
                // Replace some roofs with other materials
                match index % 7 {
                    0 | 4 => custom_palette.roof = custom_palette.floor.clone(),
                    2 | 6 => custom_palette.roof = block_palette.roof.clone(),
                    _ => (),
                }
            }

            // Generate a structure on the plot
            if let Some(new_plot) =
                structure_builder::build_house(&plot_excerpt, &plot_build_area, &custom_palette)
            {
                // TODO Enforce plot_build_area before pasting the new plot into the world?

                // If there are trees that will be affected by pasting the new plot, chop them.
                let (new_x_len, new_y_len, new_z_len) = new_plot.dim();
                for x in 0..new_x_len as i64 {
                    for y in 0..new_y_len as i64 {
                        for z in 0..new_z_len as i64 {
                            if let Some(Block::None) =  new_plot.block_at(BlockCoord(x, y, z)) {
                                // Nothing will be pasted, so nothing to do.
                            } else {
                                // Some block will be pasted, chop any affected tree.
                                tree::chop(&mut excerpt, BlockCoord(x, y, z) + bounding_box.0);
                            }
                        }
                    }
                }

                // Paste it back into the "main" excerpt
                excerpt.paste(bounding_box.0, &new_plot)
            }
        }
    }

    wall::build_wall_crowning(&mut excerpt, &wall_circle, &features, &block_palette);

    /*
    println!("Testing rainbow trees!");
    tree::rainbow_trees(&mut excerpt);
    println!("Rainbow trees finished!");
    */


    // World export
    // ************
    info!("Exporting to {:?}", output_directory);
    excerpt.to_save((x, y, z).into(), Path::new(output_directory));
    info!("Exported world excerpt of dimensions {:?}", excerpt.dim());
}

fn parse_i64_or_exit(string: &str) -> i64 {
    string.parse::<i64>().unwrap_or_else(|_| {
        error!("Not an integer: {}", string);
        std::process::exit(1);
    })
}

fn matches() -> clap::ArgMatches<'static> {
    clap::App::new("leifsbu - A Minecraft settlement generator.")
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
