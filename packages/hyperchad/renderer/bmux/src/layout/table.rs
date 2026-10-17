//! Table occupancy, shared column sizing and rich cell subtree placement.

use super::{
    BTreeMap, BTreeSet, CellRect, Container, Draft, Edges, Element, Engine, LayoutError,
    LeafMeasure, LogicalSize, MAX_EXTENT, Number, TextStyle, allocation_float, child_path,
    distributed, hidden, node_key, redistribute, signed,
};

struct Row<'a> {
    source: &'a Container,
    path: Vec<usize>,
    style: TextStyle,
}
struct Cell<'a> {
    source: &'a Container,
    path: Vec<usize>,
    style: TextStyle,
    row: usize,
    col: usize,
    rows: usize,
    cols: usize,
}

fn span(number: Option<&Number>, key: &str, limit: usize) -> Result<usize, LayoutError> {
    let invalid = || LayoutError::InvalidTableSpan { key: key.into() };
    let value = match number {
        None => 1,
        Some(Number::Integer(value)) => usize::try_from(*value).map_err(|_| invalid())?,
        Some(Number::Real(value)) => real_span(*value, limit).ok_or_else(invalid)?,
        _ => return Err(invalid()),
    };
    if value == 0 || value > limit {
        return Err(invalid());
    }
    Ok(value)
}

fn rows<'a>(
    source: &'a Container,
    path: &[usize],
    inherited: &TextStyle,
    result: &mut Vec<Row<'a>>,
) -> Result<(), LayoutError> {
    if hidden(source) {
        return Ok(());
    }
    let style = inherited.inherit(source);
    match source.element {
        Element::TR => result.push(Row {
            source,
            path: path.into(),
            style,
        }),
        Element::Table | Element::THead | Element::TBody => {
            for (index, child) in source.children.iter().enumerate() {
                rows(child, &child_path(path, index), &style, result)?;
            }
        }
        _ => {
            return Err(LayoutError::InvalidTableStructure {
                key: node_key(source, path),
            });
        }
    }
    Ok(())
}

fn prefix(sizes: &[u64], gap: u64, index: usize) -> u64 {
    sizes[..index].iter().sum::<u64>() + gap * index as u64
}
fn covered(sizes: &[u64], gap: u64, start: usize, count: usize) -> u64 {
    sizes[start..start + count].iter().sum::<u64>() + gap * count.saturating_sub(1) as u64
}
fn satisfy(sizes: &mut [u64], start: usize, count: usize, wanted: u64, gap: u64) {
    let current = covered(sizes, gap, start, count);
    if current >= wanted {
        return;
    }
    let spare = wanted - current;
    for index in 0..count {
        sizes[start + index] +=
            distributed(spare, index + 1, count) - distributed(spare, index, count);
    }
}

fn assemble<'a>(
    source: &'a Container,
    path: &[usize],
    inherited: &TextStyle,
    resolved: &mut BTreeMap<Vec<usize>, Draft<'a>>,
) -> Option<Draft<'a>> {
    if hidden(source) {
        return None;
    }
    if let Some(row) = resolved.remove(path) {
        return Some(row);
    }
    let style = inherited.inherit(source);
    let mut children: Vec<_> = source
        .children
        .iter()
        .enumerate()
        .filter_map(|(index, child)| assemble(child, &child_path(path, index), &style, resolved))
        .collect();
    let bounds = children
        .iter()
        .map(|child| child.rect)
        .reduce(CellRect::union)
        .unwrap_or_default();
    for child in &mut children {
        child.rect.x -= bounds.x;
        child.rect.y -= bounds.y;
    }
    Some(Draft {
        source,
        key: node_key(source, path),
        path: path.into(),
        rect: bounds,
        content: CellRect::new(0, 0, bounds.width, bounds.height),
        extents: LogicalSize::new(bounds.width, bounds.height),
        margin: Edges::default(),
        children,
        text: Vec::new(),
        regions: Vec::new(),
        style,
    })
}

impl<M: LeafMeasure> Engine<'_, M> {
    // Keep occupancy, column solving, row solving and source assembly in explicit order.
    #[allow(clippy::too_many_lines)]
    pub(super) fn table(
        &self,
        draft: &mut Draft<'_>,
        available: LogicalSize,
        depth: usize,
    ) -> Result<(), LayoutError> {
        let mut table_rows = Vec::new();
        rows(draft.source, &draft.path, &draft.style, &mut table_rows)?;
        let mut occupied = BTreeSet::new();
        let mut cells = Vec::new();
        let mut columns = 0;
        let mut row_count = table_rows.len();
        for (row_index, row) in table_rows.iter().enumerate() {
            let mut column: usize = 0;
            for (index, source) in row.source.children.iter().enumerate() {
                if hidden(source) {
                    continue;
                }
                let path = child_path(&row.path, index);
                let key = node_key(source, &path);
                let (rowspan, colspan) = match &source.element {
                    Element::TD { rows, columns } | Element::TH { rows, columns } => (
                        span(rows.as_ref(), &key, self.options.max_nodes)?,
                        span(columns.as_ref(), &key, self.options.max_nodes)?,
                    ),
                    _ => return Err(LayoutError::InvalidTableStructure { key }),
                };
                loop {
                    if column
                        .checked_add(colspan)
                        .is_none_or(|end| end > self.options.max_nodes)
                    {
                        return Err(LayoutError::NodeLimit);
                    }
                    if (column..column + colspan).all(|col| !occupied.contains(&(row_index, col))) {
                        break;
                    }
                    column += 1;
                }
                if row_index
                    .checked_add(rowspan)
                    .is_none_or(|end| end > self.options.max_nodes)
                    || rowspan.checked_mul(colspan).is_none_or(|slots| {
                        slots > self.options.max_nodes.saturating_sub(occupied.len())
                    })
                {
                    return Err(LayoutError::NodeLimit);
                }
                for row in row_index..row_index + rowspan {
                    for col in column..column + colspan {
                        if !occupied.insert((row, col)) {
                            return Err(LayoutError::InvalidTableSpan { key });
                        }
                    }
                }
                cells.push(Cell {
                    source,
                    path,
                    style: row.style.clone(),
                    row: row_index,
                    col: column,
                    rows: rowspan,
                    cols: colspan,
                });
                column += colspan;
                columns = columns.max(column);
                row_count = row_count.max(row_index + rowspan);
            }
        }
        if columns
            .checked_mul(row_count)
            .is_none_or(|slots| slots > self.options.max_nodes)
        {
            return Err(LayoutError::NodeLimit);
        }
        let gap_x = self
            .dimension(
                draft.source.column_gap.as_ref(),
                available.width,
                true,
                &draft.key,
            )?
            .unwrap_or(0);
        let gap_y = self
            .dimension(
                draft.source.row_gap.as_ref(),
                available.height,
                false,
                &draft.key,
            )?
            .unwrap_or(0);
        let mut widths = vec![0_u64; columns];
        let mut minimums = vec![0_u64; columns];
        let mut sorted: Vec<_> = cells.iter().collect();
        sorted.sort_by_key(|cell| cell.cols);
        for cell in sorted {
            let natural =
                self.intrinsic(cell.source, &cell.path, available, &cell.style, depth + 1)?;
            let key = node_key(cell.source, &cell.path);
            let (min, _) = self.limits(cell.source, available, true, &key)?;
            let decoration = self
                .edges(cell.source, available, false, false, &key)?
                .horizontal()
                + self
                    .edges(cell.source, available, true, false, &key)?
                    .horizontal();
            satisfy(&mut widths, cell.col, cell.cols, natural, gap_x);
            satisfy(&mut minimums, cell.col, cell.cols, min + decoration, gap_x);
        }
        let target = available
            .width
            .saturating_sub(gap_x * columns.saturating_sub(1) as u64);
        let total: u64 = widths.iter().sum();
        if total > target {
            let weights: Vec<_> = widths
                .iter()
                .map(|&width| allocation_float(width))
                .collect();
            redistribute(&mut widths, &minimums, &weights, total - target, false);
        } else if total < target && columns > 0 {
            redistribute(
                &mut widths,
                &vec![MAX_EXTENT; columns],
                &vec![1.0; columns],
                target - total,
                true,
            );
        }
        let mut heights = vec![0_u64; row_count];
        let mut measured = Vec::new();
        for cell in cells {
            let width = covered(&widths, gap_x, cell.col, cell.cols);
            let child = self
                .node(
                    cell.source,
                    &cell.path,
                    LogicalSize::new(width, available.height),
                    Some(width),
                    None,
                    &cell.style,
                    depth + 1,
                )?
                .expect("visible table cell");
            if cell.rows == 1 {
                heights[cell.row] = heights[cell.row].max(child.outer_height());
            }
            measured.push((cell, child));
        }
        measured.sort_by_key(|(cell, _)| cell.rows);
        for (cell, child) in &measured {
            satisfy(
                &mut heights,
                cell.row,
                cell.rows,
                child.outer_height(),
                gap_y,
            );
        }
        let table_width = widths.iter().sum::<u64>() + gap_x * columns.saturating_sub(1) as u64;
        let table_height = heights.iter().sum::<u64>() + gap_y * row_count.saturating_sub(1) as u64;
        let mut resolved_rows = BTreeMap::new();
        for (index, row) in table_rows.into_iter().enumerate() {
            let y = prefix(&heights, gap_y, index);
            resolved_rows.insert(
                row.path.clone(),
                Draft {
                    source: row.source,
                    path: row.path.clone(),
                    key: node_key(row.source, &row.path),
                    rect: CellRect::new(0, signed(y), table_width, heights[index]),
                    content: CellRect::new(0, 0, table_width, heights[index]),
                    extents: LogicalSize::new(table_width, heights[index]),
                    margin: Edges::default(),
                    children: Vec::new(),
                    text: Vec::new(),
                    regions: Vec::new(),
                    style: row.style,
                },
            );
        }
        for (cell, mut child) in measured {
            let x = prefix(&widths, gap_x, cell.col);
            let height = covered(&heights, gap_y, cell.row, cell.rows);
            child.rect.x = signed(x) + child.margin.left;
            child.rect.y = child.margin.top;
            let extra = height.saturating_sub(child.outer_height());
            child.rect.height += extra;
            child.content.height += extra;
            let row_path = &cell.path[..cell.path.len() - 1];
            resolved_rows
                .get_mut(row_path)
                .expect("source row")
                .children
                .push(child);
        }

        draft.children = draft
            .source
            .children
            .iter()
            .enumerate()
            .filter_map(|(index, child)| {
                assemble(
                    child,
                    &child_path(&draft.path, index),
                    &draft.style,
                    &mut resolved_rows,
                )
            })
            .collect();
        draft.extents = LogicalSize::new(table_width, table_height);
        Ok(())
    }
}

/// A real span must be finite, positive and integral, and pass the grid bound
/// before conversion. This retains the original float-to-usize saturation at
/// the architecture boundary while the caller rechecks the integer bound.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn real_span(value: f32, limit: usize) -> Option<usize> {
    let value = f64::from(value);
    let maximum = allocation_float(u64::try_from(limit).unwrap_or(u64::MAX));
    if !value.is_finite() || value < 1.0 || value.fract() != 0.0 || value > maximum {
        return None;
    }
    Some(value as usize)
}
