use mcprogedit::coordinates::BlockCoord;
use num_integer::Roots;

use std::cmp::max;

/// Draws a line in three dimensins, with vertical thickness 1 and horizontal
/// thickness `thickness`.
///
/// Works by calculating multiple lines in the same direction, at a higher resolution,
/// then scaling down to block resolution.
pub fn line(p0: &BlockCoord, p1: &BlockCoord, width: i64) -> Vec<BlockCoord> {
    line_internal(p0, p1, width, 0)
}

pub fn double_line(p0: &BlockCoord, p1: &BlockCoord, spacing: i64) -> Vec<BlockCoord> {
    line_internal(p0, p1, spacing+1, spacing)
}

fn line_internal(
    p0: &BlockCoord,
    p1: &BlockCoord,
    width: i64,
    spacing: i64,
) -> Vec<BlockCoord> {
    // For narrow lines, revert to the simpler function.
    if width <= 1 {
        return narrow_line(p0, p1);
    }

    let mut line = Vec::with_capacity(
        ((diagonal_distance(&p0, p1) + 1) * (width + 1)) as usize
    );

    // Use fixed point with given precision from here on
    const UNITS: i64 = 100;
    const HALF_UNITS: i64 = UNITS / 2;
    const Y_SHIFT: i64 = (UNITS * 2) / 3;

    // NB There is something strange going on with how things must be shifted
    // in order to get things "right". It should probably be looked into...
    let scaled_up_p0 = (*p0 * UNITS) + BlockCoord(HALF_UNITS, Y_SHIFT, HALF_UNITS);
    let scaled_up_p1 = (*p1 * UNITS) + BlockCoord(HALF_UNITS, Y_SHIFT, HALF_UNITS);

    // Get a vector corresponding to the line
    let line_direction = scaled_up_p1 - scaled_up_p0;
    // Get the orthogonal vector
    let orthogonal: BlockCoord = (line_direction.2, 0, -line_direction.0).into();
    // Scale the orthogonal vector so it is half a unit long
    let length = max(1, (orthogonal.0.pow(2) + orthogonal.2.pow(2)).sqrt());
    let unit = (orthogonal * HALF_UNITS) / length;

    let line_indexes = match spacing {
        0 => (1-width..=width-1).collect(),
        n => vec![1-n,n-1],
    };

    // Collect points from multiple parallel lines, half a block apart, within (but
    // not including) width. Since the width is in number of blocks, this means we
    // must go one less than the width to each side from the center line.
    for i in line_indexes {
        // Use more probe points along the line, in order to cover all blocks.
        // Currently using double the amount of points by using HALF_UNITS instead of UNITS
        // for the scaling. It may be possible to reduce overlap by increasing the scaling
        // factor.
        let scaled_up_lines = sparse_line(
            &(scaled_up_p0 + i * unit),
            &(scaled_up_p1 + i * unit),
            HALF_UNITS,
        );

        let mut lines: Vec<BlockCoord> = scaled_up_lines.iter()
            .map(|coord| {*coord / UNITS})
            .collect();
        line.append(&mut lines);
    }

    line
}

fn sparse_line(p0: &BlockCoord, p1: &BlockCoord, step_size: i64) -> Vec<BlockCoord> {
    let n = diagonal_distance(&p0, &p1) / step_size;
    let mut points = Vec::with_capacity(n as usize + 1);

    for step in 0..=n {
        points.push(lerp_point(&p0, &p1, step, n));
    }

    points
}

// Line function and sub-functions ported from JavaScript examples on
// https://www.redblobgames.com/grids/line-drawing.html
pub fn narrow_line(p0: &BlockCoord, p1: &BlockCoord) -> Vec<BlockCoord> {
    let n = diagonal_distance(&p0, &p1);
    let mut points = Vec::with_capacity(n as usize + 1);

    for step in 0..=n {
        points.push(lerp_point(p0, p1, step, n));
    }

    points
}

fn diagonal_distance(p0: &BlockCoord, p1: &BlockCoord) -> i64 {
    let line_vector = *p1 - *p0;
    max(max(line_vector.0.abs(), line_vector.1.abs()), line_vector.2.abs())
}

fn lerp(start: i64, end: i64, step: i64, n: i64) -> i64 {
    if n == 0 {
        0
    } else {
        start + step * (end - start) / n
    }
}

fn lerp_point(p0: &BlockCoord, p1: &BlockCoord, step: i64, n: i64) -> BlockCoord {
    BlockCoord(
        lerp(p0.0, p1.0, step, n),
        lerp(p0.1, p1.1, step, n),
        lerp(p0.2, p1.2, step, n),
    )
}
