use std::f64::consts::TAU;

use egui::{Align2, Color32, RichText, Stroke};
use egui_plot::{Legend, Plot, PlotPoint, PlotPoints, Polygon, Text};

const FULL_CIRCLE_VERTICES: f64 = 360.0;
const RADIUS: f64 = 1.0;

pub(crate) struct PieChart {
    name: String,
    sectors: Vec<Sector>,
}

impl PieChart {
    pub fn new(name: String, data: Vec<(f64, String, Color32)>) -> Self {
        let sum: f64 = data.iter().map(|(number, _, _)| number).sum();

        let slices: Vec<_> = data
            .into_iter()
            .filter_map(|(number, name, color)| {
                if number == 0.0 {
                    None
                } else {
                    Some((number, number / sum, name, color))
                }
            })
            .collect();

        let step = TAU / FULL_CIRCLE_VERTICES;

        let mut offset = 0.0_f64;

        let sectors = slices
            .into_iter()
            .map(|(number, percent, name, color)| {
                let vertices = (FULL_CIRCLE_VERTICES * percent).floor() as usize;

                let start = TAU * offset;
                let end = TAU * (offset + percent);

                let sector = Sector::new(number, name, start, end, vertices, step, color);

                offset += percent;

                sector
            })
            .collect();

        Self {
            name,
            sectors,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let sectors = self.sectors.clone();

        Plot::new(&self.name)
            .label_formatter(|_: &str, _: &PlotPoint| String::default())
            .show_background(false)
            .legend(Legend::default())
            .show_axes([false, false])
            .show_grid(false)
            .allow_boxed_zoom(false)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .data_aspect(1.0)
            // 显示范围略大于圆半径
            .include_x(-1.1 * RADIUS)
            .include_x(1.1 * RADIUS)
            .include_y(-1.1 * RADIUS)
            .include_y(1.1 * RADIUS)
            .height(150.0)
            .width(285.0)
            .show(ui, |plot_ui| {
                for sector in sectors.into_iter() {
                    let highlight = plot_ui
                        .pointer_coordinate()
                        .map(|p| sector.contains(&p))
                        .unwrap_or_default();

                    let Sector {
                        name,
                        number,
                        points,
                        color,
                        ..
                    } = sector;

                    plot_ui.polygon(
                        Polygon::new(PlotPoints::new(points))
                            .name(&name)
                            .fill_color(Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 200))
                            .stroke(Stroke { width: 1.5, color })
                            .highlight(false),
                    );

                    if highlight {
                        let mut point = plot_ui.pointer_coordinate().unwrap();
                        // 防止文字与轴线重叠
                        point = PlotPoint::new(point.x + 0.07, point.y - 0.07);

                        let text = RichText::new(format!(" {}\n {}", name, number)).size(15.0);

                        plot_ui.text(
                            Text::new(point, text)
                                .name(&name)
                                .anchor(Align2::LEFT_TOP)
                                // 解决字体被填充颜色覆盖问题
                                .highlight(true),
                        );
                    }
                }
            });
    }
}

#[derive(Clone)]
struct Sector {
    name: String,
    number: f64,
    start: f64,
    end: f64,
    points: Vec<[f64; 2]>,
    color: Color32,
}

impl Sector {
    pub fn new<S: AsRef<str>>(
        number: f64,
        name: S,
        start: f64,
        end: f64,
        vertices: usize,
        step: f64,
        color: Color32,
    ) -> Self {
        // 计算扇形点位
        let mut points = vec![];

        // 若扇形未占满整个圆，则增加圆心点位
        if end - TAU != start {
            points.push([0.0, 0.0]);
        }

        // 圆弧坐标点
        points.push([RADIUS * start.sin(), RADIUS * start.cos()]);
        for v in 1..vertices {
            let t: f64 = start + step * v as f64;
            points.push([RADIUS * t.sin(), RADIUS * t.cos()]);
        }
        points.push([RADIUS * end.sin(), RADIUS * end.cos()]);

        Self {
            name: name.as_ref().to_string(),
            number,
            start,
            end,
            points,
            color,
        }
    }

    /// 用于判断鼠标所指的点是否在某一部分扇形区间内
    pub fn contains(&self, &PlotPoint { x, y }: &PlotPoint) -> bool {
        let r = y.hypot(x);
        let mut theta = x.atan2(y);

        if theta < 0.0 {
            theta += TAU;
        }

        r < RADIUS && theta > self.start && theta < self.end
    }
}