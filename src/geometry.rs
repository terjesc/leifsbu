use mcprogedit::coordinates::BlockColumnCoord;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeftRightSide {
    Left,
    On,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InOutSide {
    Inside,
    On,
    Outside,
}

/// For a line through `line.0` and `line.1`, in that direction,
/// which side is `point` on relative to the line?
pub fn point_position_relative_to_line(
    point: BlockColumnCoord,
    line: (BlockColumnCoord, BlockColumnCoord),
) -> LeftRightSide {
    let double_area = (line.1 .0 - line.0 .0) * (point.1 - line.0 .1)
        - (point.0 - line.0 .0) * (line.1 .1 - line.0 .1);

    if double_area > 0 {
        LeftRightSide::Left
    } else if double_area < 0 {
        LeftRightSide::Right
    } else {
        LeftRightSide::On
    }
}

/// For a (counter-clockwise) `polygon`, is `point` inside or outside of the polygon?
pub fn point_position_relative_to_polygon(
    point: BlockColumnCoord,
    polygon: &[BlockColumnCoord],
) -> InOutSide {
    let winding_number = polygon.windows(2).fold(0i64, |winding_number, line| {
        if line[0].1 <= point.1 {
            if line[1].1 > point.1
                && LeftRightSide::Left == point_position_relative_to_line(point, (line[0], line[1]))
            {
                winding_number + 1
            } else {
                winding_number
            }
        } else {
            if line[1].1 <= point.1
                && LeftRightSide::Right
                    == point_position_relative_to_line(point, (line[0], line[1]))
            {
                winding_number - 1
            } else {
                winding_number
            }
        }
    });

    if 0 == winding_number {
        InOutSide::Outside
    } else {
        InOutSide::Inside
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_left_of_line() {
        assert_eq!(
            LeftRightSide::Left,
            point_position_relative_to_line(
                BlockColumnCoord(-2, -2),
                (BlockColumnCoord(3, -4), BlockColumnCoord(1, 2)),
            ),
        );
    }

    #[test]
    fn point_right_of_line() {
        assert_eq!(
            LeftRightSide::Right,
            point_position_relative_to_line(
                BlockColumnCoord(4, 1),
                (BlockColumnCoord(3, -4), BlockColumnCoord(1, 2)),
            ),
        );
    }

    #[test]
    fn point_on_line() {
        assert_eq!(
            LeftRightSide::On,
            point_position_relative_to_line(
                BlockColumnCoord(2, -1),
                (BlockColumnCoord(3, -4), BlockColumnCoord(1, 2)),
            ),
        );
    }

    #[test]
    fn point_inside_polygon() {
        assert_eq!(
            InOutSide::Inside,
            point_position_relative_to_polygon(
                BlockColumnCoord(2, -1),
                &[
                    BlockColumnCoord(3, -4),
                    BlockColumnCoord(4, 1),
                    BlockColumnCoord(1, 2),
                    BlockColumnCoord(-2, -2),
                ],
            ),
        );
    }

    #[test]
    fn point_outside_polygon() {
        assert_eq!(
            InOutSide::Outside,
            point_position_relative_to_polygon(
                BlockColumnCoord(3, -4),
                &[
                    BlockColumnCoord(2, -1),
                    BlockColumnCoord(4, 1),
                    BlockColumnCoord(1, 2),
                    BlockColumnCoord(-2, -2),
                ],
            ),
        );
    }
}
