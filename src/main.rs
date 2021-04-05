//! Leifsbudir - settlement generator for Minecraft

extern crate clap;
extern crate mcprogedit;

mod areas;
mod features;
mod walled_town;

use std::path::Path;

use mcprogedit::block::Block;
use mcprogedit::world_excerpt::WorldExcerpt;

use crate::areas::*;
use crate::features::*;
use crate::walled_town::*;

fn main() {
    // Read arguments
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

    // Import the given area from the given save file directory
    println!("Importing from {:?}", input_directory);
    let mut excerpt = WorldExcerpt::from_save(
        (x, y, z).into(),
        (x + x_len - 1, y + y_len - 1, z + z_len - 1).into(),
        Path::new(input_directory),
    );
    println!("Imported world excerpt of dimensions {:?}", excerpt.dim());

    // Extract features
    let features = Features::new_from_world_excerpt(&excerpt);

    // Find areas suitable for various purposes (based on features)
    let areas = Areas::new_from_features(&features);

    let town_circumference = walled_town_contour(&features, &areas);

    for (x, z) in town_circumference {
        // TODO place pillar
        let ground = features.terrain_height_map.height_at((x, z)).unwrap() as i64;
        for y in ground..ground + 3 {
            excerpt.set_block_at((x as i64, y, z as i64).into(), Block::Cobblestone);
        }
        excerpt.set_block_at((x as i64, ground + 4, z as i64).into(), Block::torch());
    }

    // TODO Generate area plan (based on areas found above)
    // - start by putting down primary sector areas
    // - put down some housing close to primary sector areas
    // - put down some secondary sector areas:
    //      * in proximity to primary sector areas
    //      * in proximity to sea or land routes
    // - put down more housing in proximity to secondary sector areas
    // - put down tertiary sector areas among densest housing areas
    //
    // - put down major roads connecting to the "centers" or "edges" of the areas
    //      * maybe defer tertiary to now? Put where traffic is highest...

    // TODO Generate city plan (based on area plan)
    // - minor roads
    // - individual plots / buildings

    // TODO Generate structures (based on city plan)
    // (NOTE: This step modifies the excerpt.)

    // Export the modified world excerpt to the given save file directory
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
