//! Leifsbudir - settlement generator for Minecraft

extern crate clap;
extern crate mcprogedit;

mod areas;
mod build_area;
mod features;
mod geometry;
mod line;
mod partitioning;
mod pathfinding;
mod plot;
mod road;
mod structure_builder;
mod tree;
mod types;
mod wall;
mod walled_town;

use std::path::Path;

use imageproc::stats::histogram;
use mcprogedit::coordinates::{BlockColumnCoord, BlockCoord};
use mcprogedit::world_excerpt::WorldExcerpt;

use crate::areas::*;
use crate::features::*;
use crate::geometry::{extract_blocks, LandUsageGraph};
use crate::partitioning::divide_town_into_blocks;
use crate::plot::divide_city_block;
use crate::road::roads_split;
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
    // - Town is complicated. Can to some extent displace fields/livestock/forest

    // Find town location
    let (town_circumference, town_center) = walled_town_contour(&features, &areas);

    // Create some paths... (NB Only for generating a cool image. Not built in world.)
    let start = town_center;
    let mut path_image = features.coloured_map.clone();

    for goal in &town_circumference {
        if let Some(path) = pathfinding::path(start, *goal, &features.terrain) {
            draw_snake(&mut path_image, &path);
        }
    }
    path_image.save("path_001.png").unwrap();

    // Get full wall circle, by copying the first node of the wall to the end.
    let mut wall_circle = town_circumference.clone();
    wall_circle.push(town_circumference[0]);

    // TODO
    // - Find primary sector areas (agriculture, fishing, forestry, mining)
    // - Put major roads from primary sectors to town circumference

    // Create road paths...
    // TODO refactor: Move the path generation somewhere else?
    // TODO to be replaced by other means of finding road start locations
    let start_coordinates: Vec<_> = vec![
        (0, 0),
        (0, z_len - 1),
        (x_len - 1, z_len - 1),
        (x_len - 1, 0),
    ]
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

    let mut land_usage_graph = LandUsageGraph::new();
    land_usage_graph.add_roads(&streets, geometry::EdgeKind::Street, 2);
    land_usage_graph.add_roads(&city_roads, geometry::EdgeKind::Road, 4);
    land_usage_graph.add_circumference(&wall_circle, geometry::EdgeKind::Wall, 3);

    // Get the polygons for each "city block"
    let districts = extract_blocks(&land_usage_graph);

    // Make images of the extracted city blocks (for debug visuals only)
    for (colour, district) in districts.iter().enumerate() {
        let mut district_image = image::ImageBuffer::new(x_len as u32, z_len as u32);
        geometry::draw_area(
            &mut district_image,
            &district,
            BlockColumnCoord(0, 0),
            image::Luma([63u8]),
        );
        partitioning::draw_offset_snake(
            &mut district_image,
            &district,
            &BlockColumnCoord(0, 0),
            image::Luma([255u8]),
        );
        district_image.save(format!("D-01 district {:0>2}.png", colour)).unwrap();
        println!("District {} has area {}.", colour, geometry::area(&district));
    
        let stats = histogram(&district_image);
        let surface_area = stats.channels[0][63];
        let border_area = stats.channels[0][255];
        println!(
            "District {} image areas: {} + ({} / 2) = {}",
            colour, surface_area, border_area, surface_area + (border_area / 2)
        );
    }
    //district_image.save("D-01 districts.png").unwrap();

    // Split the city blocks
    let mut plots = Vec::new();
    for district in districts {
        let mut district_plots = divide_city_block(&district, &land_usage_graph);
        // TODO draw the plots or something...
        println!("Found {} plots for a district.", district_plots.len());
        plots.append(&mut district_plots);
    }

    let mut city_plan = features.coloured_map.clone();
    for plot in &plots {
        plot.draw(&mut city_plan);
    }
    /*
    for street in &streets {
        pathfinding::draw_road_path(&mut city_plan, street);
    }
    */
    for road in &country_roads {
        pathfinding::draw_road_path(&mut city_plan, road);
    }
    /*
    for road in &city_roads {
        pathfinding::draw_road_path(&mut city_plan, road);
    }
    */
    city_plan.save("city plan.png").unwrap();


    // Build structures
    // ****************

    // Build that wall! (But who is going to pay for it?)
    wall::build_wall(&mut excerpt, &wall_circle, &features);

    // Build the various roads and streets...
    for street in streets {
        road::build_road(&mut excerpt, &street, &features.terrain, 2);
    }

    for road in country_roads {
        road::build_road(&mut excerpt, &road, &features.terrain, 3);
    }

    for road in city_roads {
        road::build_road(&mut excerpt, &road, &features.terrain, 4);
    }

    // Build some structures (houses?) on the plots.
    for plot in &plots {
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

            // Generate a structure on the plot
            let new_plot = structure_builder::build_rock(&plot_excerpt, &plot_build_area);
            // TODO "clean up" new_plot before actually saving it into excerpt?

            // Paste it back into the "main" excerpt
            excerpt.paste(bounding_box.0, &new_plot)
        }
    }

    // TODO
    // - If player location is inside town, not on road, then make square plot there
    // - If player location is outside town, make road from there to nearest major road,
    //   and put signs towards town. Bridges, boat trips, etc. may be needed...

    /*
    println!("Testing rainbow trees!");
    tree::rainbow_trees(&mut excerpt);
    println!("Rainbow trees finished!");
    */


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
