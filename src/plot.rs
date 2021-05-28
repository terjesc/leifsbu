use crate::geometry;
use crate::geometry::{IntersectionPoints, LandUsageGraph, RawEdge2d, RawEdge3d};
use imageproc::drawing::draw_line_segment_mut;
use mcprogedit::coordinates::{BlockColumnCoord, BlockCoord};

const PLOT_AREA_MIN: i64 = 40;
const PLOT_AREA_MAX: i64 = 200;

#[derive(Clone, Debug)]
pub struct Plot {
    pub edges: Vec<PlotEdge>,
}

#[derive(Clone, Copy, Debug)]
pub struct PlotEdge {
    pub kind: PlotEdgeKind,
    pub points: RawEdge3d,
}

#[derive(Clone, Copy, Debug)]
pub enum PlotEdgeKind {
    Road { width: usize },
    Wall { width: usize },
    Plot,
    Terrain,
}

impl Plot {
    pub fn polygon(&self) -> Vec<BlockColumnCoord> {
        let mut polygon = Vec::new();

        if let Some(edge) = self.edges.first() {
            polygon.push(BlockColumnCoord::from(edge.points.0));
        }

        for edge in &self.edges {
            polygon.push(BlockColumnCoord::from(edge.points.1));
        }

        polygon
    }

    pub fn bounding_box(&self) -> Option<(BlockCoord, BlockCoord)> {
        if self.edges.is_empty() {
            return None;
        }

        let mut min = self.edges[0].points.0;
        let mut max = self.edges[0].points.0;

        for edge in &self.edges {
            let end_point = edge.points.1;
            min = BlockCoord(
                i64::min(end_point.0, min.0),
                i64::min(end_point.1, min.1),
                i64::min(end_point.2, min.2),
            );
            max = BlockCoord(
                i64::max(end_point.0, min.0),
                i64::max(end_point.1, min.1),
                i64::max(end_point.2, min.2),
            );
        }

        Some((min, max))
    }

    pub fn offset(&self, offset: BlockCoord) -> Self {
        let mut edges = Vec::new();

        for edge in &self.edges {
            edges.push(
                PlotEdge {
                    kind: edge.kind,
                    points: (
                        edge.points.0 - offset,
                        edge.points.1 - offset,
                    ),
                }
            );
        }

        Self { edges }
    }

    pub fn point_slice(&self) -> Vec<imageproc::point::Point<i64>> {
        let point_vec: Vec<imageproc::point::Point<i64>> = self
            .polygon()
            .iter()
            .map(|coordinates| imageproc::point::Point::<i64>::new(coordinates.0, coordinates.1))
            .collect();

        point_vec
    }

    pub fn split(&self, split_line: &RawEdge2d) -> (Plot, Plot) {
        let mut edges_0 = Vec::<PlotEdge>::new();
        let mut edges_1 = Vec::<PlotEdge>::new();

        enum State {
            InitialFirstPlot,
            SecondPlot,
            FinalFirstPlot,
        }

        let mut state = State::InitialFirstPlot;

        // Figure out what "side" each plot is on.
        let plot_0_side = match geometry::point_position_relative_to_line(
            self.edges[0].points.0.into(),
            *split_line,
        ) {
            // NB picking Left if exactly on the split line, but this is probably
            //    too naïve and going backwards through the edges until we reach
            //    an actual side (and use that one) is probably better.
            geometry::LeftRightSide::On => geometry::LeftRightSide::Left,
            side => side,
        };

        let plot_1_side = match plot_0_side {
            geometry::LeftRightSide::Left => geometry::LeftRightSide::Right,
            geometry::LeftRightSide::Right => geometry::LeftRightSide::Left,
            _ => unreachable!(),
        };

        // NB This takes the first split encountered. There may be several splits,
        //    and a different split may be "better", e.g. with respect to balancing
        //    the areas of the resulting plots, keeping road access, etc.
        for edge in &self.edges {
            match state {
                State::InitialFirstPlot => {
                    let edge_segment = (edge.points.0.into(), edge.points.1.into());
                    match geometry::intersection(edge_segment, *split_line) {
                        IntersectionPoints::None | IntersectionPoints::Two(_, _) => {
                            // No intersection, or still not completely crossed the split line.
                            // Continue with the first plot.
                            edges_0.push(edge.clone())
                        }
                        IntersectionPoints::One(coordinates) => {
                            if coordinates == edge_segment.1 {
                                // Reached the split line, but not crossed yet.
                                edges_0.push(edge.clone());
                            } else if coordinates == edge_segment.0 {
                                // Left the split line. Have crossed if the edge endpoint
                                // is not on the plot 0 side.
                                let edge_endpoint_side = geometry::point_position_relative_to_line(
                                    edge_segment.1.into(),
                                    *split_line,
                                );

                                if edge_endpoint_side == plot_0_side {
                                    // Still on plot 0
                                    edges_0.push(edge.clone());
                                } else {
                                    // Crossed over to plot 1
                                    edges_1.push(edge.clone());
                                    state = State::SecondPlot;
                                }
                            } else {
                                // The edge fully bridges the split line.

                                // NB arithmetic mean is not correct here,
                                //    should interpolate between the points instead...

                                // Find the full 3d coordinates for the intersection point
                                let y = (edge.points.0 .1 + edge.points.1 .1) / 2;
                                let full_coordinates = BlockCoord(coordinates.0, y, coordinates.1);

                                // Add the split edge to respective plots
                                edges_0.push(PlotEdge {
                                    kind: edge.kind,
                                    points: (edge.points.0, full_coordinates),
                                });
                                edges_1.push(PlotEdge {
                                    kind: edge.kind,
                                    points: (full_coordinates, edge.points.1),
                                });
                                state = State::SecondPlot;
                            }
                        }
                    }
                }
                State::SecondPlot => {
                    // NB this state is copy-pasted from State::InitialFirstPlot,
                    //    with edge/plot handling flipped and some extra edges added.
                    let edge_segment = (edge.points.0.into(), edge.points.1.into());
                    match geometry::intersection(edge_segment, *split_line) {
                        IntersectionPoints::None | IntersectionPoints::Two(_, _) => {
                            // No intersection, or still not completely crossed the split line.
                            // Continue with the first plot.
                            edges_1.push(edge.clone())
                        }
                        IntersectionPoints::One(coordinates) => {
                            if coordinates == edge_segment.1 {
                                // Reached the split line, but not crossed yet.
                                edges_1.push(edge.clone());
                            } else if coordinates == edge_segment.0 {
                                // Left the split line. Have crossed if the edge endpoint
                                // is not on the plot 0 side.
                                let edge_endpoint_side = geometry::point_position_relative_to_line(
                                    edge_segment.1.into(),
                                    *split_line,
                                );

                                if edge_endpoint_side == plot_1_side {
                                    // Still on plot 1
                                    edges_1.push(edge.clone());
                                } else {
                                    // Crossed over to plot 0
                                    // Add new edges along the split line.
                                    edges_1.push(PlotEdge {
                                        kind: PlotEdgeKind::Plot,
                                        points: (edge.points.0, edges_1[0].points.0),
                                    });
                                    edges_0.push(PlotEdge {
                                        kind: PlotEdgeKind::Plot,
                                        points: (edges_1[0].points.0, edge.points.0),
                                    });
                                    // Add edge to plot 0
                                    edges_0.push(edge.clone());
                                    state = State::FinalFirstPlot;
                                }
                            } else {
                                // The edge fully bridges the split line.

                                // NB arithmetic mean is not correct here,
                                //    should interpolate between the points instead...

                                // Find the full 3d coordinates for the intersection point
                                let y = (edge.points.0 .1 + edge.points.1 .1) / 2;
                                let full_coordinates = BlockCoord(coordinates.0, y, coordinates.1);

                                // Add part of edge belonging to plot 1.
                                edges_1.push(PlotEdge {
                                    kind: edge.kind,
                                    points: (edge.points.0, full_coordinates),
                                });
                                // Add new edges along split line.
                                edges_1.push(PlotEdge {
                                    kind: PlotEdgeKind::Plot,
                                    points: (full_coordinates, edges_1[0].points.0),
                                });
                                edges_0.push(PlotEdge {
                                    kind: PlotEdgeKind::Plot,
                                    points: (edges_1[0].points.0, full_coordinates),
                                });
                                // Add part of edge belonging to plot 0.
                                edges_0.push(PlotEdge {
                                    kind: edge.kind,
                                    points: (full_coordinates, edge.points.1),
                                });
                                state = State::FinalFirstPlot;
                            }
                        }
                    }
                }
                State::FinalFirstPlot => {
                    // Add to edges_0
                    edges_0.push(edge.clone());
                }
            }
        }

        (Plot { edges: edges_0 }, Plot { edges: edges_1 })
    }

    pub fn has_access(&self) -> bool {
        for edge in &self.edges {
            match edge.kind {
                PlotEdgeKind::Road { .. } => return true,
                _ => (),
            }
        }
        false
    }

    pub fn draw(&self, image: &mut image::RgbImage) {
        for edge in &self.edges {
            let colour = match edge.kind {
                PlotEdgeKind::Road { .. } => image::Rgb([191u8, 63u8, 63u8]),
                PlotEdgeKind::Wall { .. } => image::Rgb([63u8, 63u8, 63u8]),
                PlotEdgeKind::Plot => image::Rgb([127u8, 255u8, 127u8]),
                PlotEdgeKind::Terrain => image::Rgb([0u8, 127u8, 127u8]),
            };
            draw_line_segment_mut(
                image,
                (edge.points.0 .0 as f32, edge.points.0 .2 as f32),
                (edge.points.1 .0 as f32, edge.points.1 .2 as f32),
                colour,
            );
        }
    }
}

pub fn divide_city_block(
    city_block: &Vec<BlockColumnCoord>,
    land_usage: &LandUsageGraph,
) -> Vec<Plot> {
    let plot = land_usage.plot_from_area(city_block);
    divide_plot(&plot)
}

fn divide_plot(plot: &Plot) -> Vec<Plot> {
    rec_subdiv_obb(plot, (PLOT_AREA_MIN, PLOT_AREA_MAX))
}

fn rec_subdiv_obb(plot: &Plot, area_bounds: (i64, i64)) -> Vec<Plot> {
    //println!("rec_subdiv_obb()");
    let polygon = plot.polygon();
    let area = geometry::area(&polygon);

    // Do not split if already small enough
    if area < area_bounds.1 {
        //println!("Area already satisfactory. Aborting.");
        return vec![plot.clone()];
    }

    // NB May add front side width constraint, similar to area constraint above.

    // Get potential split lines
    let (short_edge, long_edge) = compute_split_lines(plot);

    // Split the plot
    let (plot_1, plot_2) = {
        //println!("Splitting along the short edge.");
        let (short_plot_1, short_plot_2) = plot.split(&short_edge);
        if short_plot_1.has_access() && short_plot_2.has_access() {
            (short_plot_1, short_plot_2)
        } else {
            //println!("Splitting along the long edge instead.");
            let (long_plot_1, long_plot_2) = plot.split(&long_edge);
            if long_plot_1.has_access() && long_plot_2.has_access() {
                (long_plot_1, long_plot_2)
            } else {
                //println!("Couldn't keep road access. Aborting.");
                return vec![plot.clone()];
            }
        }
    };

    // Build the output from recurring on the two plots from the split
    let mut plots = rec_subdiv_obb(&plot_1, area_bounds);
    plots.append(&mut rec_subdiv_obb(&plot_2, area_bounds));

    plots
}

/// Find potential split lines for the plot, from the Oriented Bounding Box (OBB).
fn compute_split_lines(plot: &Plot) -> (RawEdge2d, RawEdge2d) {
    let point_slice = plot.point_slice();
    let obb = imageproc::geometry::min_area_rect(&point_slice);

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
