use crate::features::Features;

use image::GrayImage;
use image::imageops::*;
use imageproc::*;
use imageproc::distance_transform::Norm;

//const TOWN_FLATNESS_TRESHOLD: u8 = 56;
const TOWN_FLATNESS_TRESHOLD: u8 = 64;
const TOWN_DISTANCE_INTO_WATER: u8 = 2;
const WOOD_CONNECTEDNESS_TRESHOLD: u8 = 5;
const AGRICULTURE_FLATNESS_TRESHOLD: u8 = 32;

pub struct Areas {
    pub town: GrayImage,
    pub woodcutters: GrayImage,
    pub _agriculture: GrayImage,
    pub _agriculture_without_trees: GrayImage,
    //pub harbour: GrayImage,
    //pub mines: GrayImage,
    //pub fishers: GrayImage,
    //pub town_road: GrayImage, // decide as part of town area instead?
    //pub lighthouse: GrayImage, // decide as part of harbour/fishers instead?
    //pub squares: GrayImage, // decide as part of town area instead?
    //pub fortifications: GrayImage,
}

impl Areas {
    pub fn new_from_features(features: &Features) -> Self {
        let town = Self::town(features);
        let woodcutters = Self::woodcutters(features);
        let (_agriculture, _agriculture_without_trees) = Self::agriculture(features);

        Self {
            town,
            woodcutters,
            _agriculture,
            _agriculture_without_trees,
        }
    }

    fn town(features: &Features) -> GrayImage {
        // Suitable area for "town":
        // * on land, or a couple of blocks into water
        let mut land_mask = features.water.clone();
        invert(&mut land_mask);
        morphology::close_mut(&mut land_mask, Norm::L1, TOWN_DISTANCE_INTO_WATER);
        morphology::open_mut(&mut land_mask, Norm::L1, TOWN_DISTANCE_INTO_WATER);
        //land_mask.save("A-01a land mask.png").unwrap();
        // * reasonably flat
        let mut flat_mask = contrast::threshold(&features.scharr, TOWN_FLATNESS_TRESHOLD);
        invert(&mut flat_mask);
        //flat_mask.save("A-01b flat mask.png").unwrap();

        // * not full of trees
        /* Uncomment this code for avoiding building cities on forests.
        let mut forest_mask = Self::woodcutters(features);
        invert(&mut forest_mask);
        morphology::dilate_mut(&mut forest_mask, Norm::LInf, 5u8);
        */

        //distance_transform_mut(&mut forest_mask, Norm::LInf);
        //threshold_mut(&mut forest_mask, 6u8);
        //invert(&mut forest_mask);
        //forest_mask.save("A-01c forest mask.png").unwrap();

        // Intersection of masks is suitable for town
        let (x_len, z_len) = features.dimensions();
        let mut town = image::ImageBuffer::new(x_len as u32, z_len as u32);
        for x in 0..x_len as u32 {
            for z in 0..z_len as u32 {
                if image::Luma([255u8]) == land_mask[(x, z)]
                && image::Luma([255u8]) == flat_mask[(x, z)]
                //&& image::Luma([255u8]) == forest_mask[(x, z)] // Uncomment for avoiding building cities on forests.
                {
                    town.put_pixel(x, z, image::Luma([255u8]));
                }
            }
        }

        #[cfg(feature = "debug_images")]
        town.save("A-01 town.png").unwrap();

        town
    }

    fn woodcutters(features: &Features) -> GrayImage {
        let mut woodcutters = features.forest.clone();
        morphology::close_mut(
            &mut woodcutters,
            Norm::L1,
            WOOD_CONNECTEDNESS_TRESHOLD,
        );
        morphology::open_mut(
            &mut woodcutters,
            Norm::L1,
            2 * WOOD_CONNECTEDNESS_TRESHOLD,
        );

        #[cfg(feature = "debug_images")]
        woodcutters.save("A-02 woodcutters.png").unwrap();

        woodcutters
    }

    fn agriculture(features: &Features) -> (GrayImage, GrayImage) {
        // Suitable area for "agriculture":
        // * fertile land
        // * not under water
        // * not too many trees
        // * not too steep

        let (x_len, z_len) = features.dimensions();

        let mut agriculture = features.fertile.clone();
        let steep_mask = contrast::threshold(&features.scharr, AGRICULTURE_FLATNESS_TRESHOLD);

        for x in 0..x_len as u32 {
            for z in 0..z_len as u32 {
                if image::Luma([255u8]) == steep_mask[(x, z)]
                    || image::Luma([255u8]) == features.snow[(x, z)] {
                    agriculture.put_pixel(x, z, image::Luma([0u8]));
                }
            }
        }

        let mut agriculture_without_trees = agriculture.clone();

        for x in 0..x_len as u32 {
            for z in 0..z_len as u32 {
                if image::Luma([255u8]) == features.forest[(x, z)] {
                    agriculture_without_trees.put_pixel(x, z, image::Luma([0u8]));
                }
            }
        }

        #[cfg(feature = "debug_images")]
        {
            agriculture.save("A-03 agriculture.png").unwrap();
            agriculture_without_trees.save("A-04 agriculture without trees.png").unwrap();
        }

        (agriculture, agriculture_without_trees)
    }
}
