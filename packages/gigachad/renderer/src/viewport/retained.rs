use std::sync::Arc;

pub trait WidgetPosition: Send + Sync {
    fn widget_x(&self) -> i32;
    fn widget_y(&self) -> i32;
    fn widget_w(&self) -> i32;
    fn widget_h(&self) -> i32;
}

impl std::fmt::Debug for dyn WidgetPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "({}, {}, {}, {})",
            self.widget_x(),
            self.widget_y(),
            self.widget_w(),
            self.widget_h()
        ))
    }
}

#[derive(Clone)]
pub struct Viewport {
    widget: Arc<Box<dyn WidgetPosition>>,
    parent: Option<Box<Viewport>>,
    position: Arc<Box<dyn ViewportPosition + Send + Sync>>,
}

impl std::fmt::Debug for Viewport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut binding = f.debug_struct("Viewport");
        let x = binding
            .field("x", &self.x())
            .field("y", &self.y())
            .field("w", &self.w())
            .field("h", &self.h());

        if let Some(parent) = &self.parent {
            x.field("parent", &parent);
        }

        x.finish_non_exhaustive()
    }
}

impl Viewport {
    pub fn new(
        parent: Option<Self>,
        position: impl ViewportPosition + Send + Sync + 'static,
    ) -> Self {
        Self {
            widget: Arc::new(position.as_widget_position()),
            parent: parent.map(Box::new),
            position: Arc::new(Box::new(position)),
        }
    }

    fn x(&self) -> i32 {
        self.position.viewport_x()
    }

    fn y(&self) -> i32 {
        self.position.viewport_y()
    }

    fn w(&self) -> i32 {
        self.position.viewport_w()
    }

    fn h(&self) -> i32 {
        self.position.viewport_h()
    }

    fn is_widget_visible(&self, widget: &dyn WidgetPosition) -> (bool, u32) {
        let (visible_in_current_viewport, dist) =
            self.position.is_widget_visible(&**self.widget, widget);

        // FIXME: This doesn't correctly check the position leaf widget (the param above)
        // within this viewport itself, but this probably isn't a huge issue since nested
        // `Viewport`s isn't super likely yet.
        if visible_in_current_viewport {
            self.parent
                .as_ref()
                .map_or((visible_in_current_viewport, dist), |parent| {
                    let (parent_visible, parent_dist) = parent.is_widget_visible(&**self.widget);

                    (
                        visible_in_current_viewport && parent_visible,
                        dist + parent_dist,
                    )
                })
        } else {
            (false, dist)
        }
    }
}

#[allow(clippy::module_name_repetitions)]
pub trait ViewportPosition {
    fn viewport_x(&self) -> i32;
    fn viewport_y(&self) -> i32;
    fn viewport_w(&self) -> i32;
    fn viewport_h(&self) -> i32;
    fn as_widget_position(&self) -> Box<dyn WidgetPosition>;

    fn is_widget_visible(
        &self,
        this_widget: &dyn WidgetPosition,
        widget: &dyn WidgetPosition,
    ) -> (bool, u32) {
        #[allow(clippy::cast_precision_loss)]
        let (visible, dist) = super::is_visible(
            this_widget.widget_x() as f32,
            this_widget.widget_y() as f32,
            self.viewport_w() as f32,
            self.viewport_y() as f32,
            widget.widget_x() as f32,
            widget.widget_y() as f32,
            widget.widget_w() as f32,
            widget.widget_h() as f32,
        );

        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        (visible, dist.round() as u32)
    }
}

impl std::fmt::Debug for dyn ViewportPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewportPosition")
            .field("x", &self.viewport_x())
            .field("y", &self.viewport_y())
            .field("w", &self.viewport_w())
            .field("h", &self.viewport_h())
            .finish()
    }
}

impl std::fmt::Debug for Box<dyn ViewportPosition + Send + Sync> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewportPosition")
            .field("x", &self.viewport_x())
            .field("y", &self.viewport_y())
            .field("w", &self.viewport_w())
            .field("h", &self.viewport_h())
            .finish()
    }
}

#[allow(clippy::module_name_repetitions)]
pub struct ViewportListener {
    widget: Box<dyn WidgetPosition>,
    viewport: Option<Viewport>,
    visible: bool,
    dist: u32,
    callback: Box<dyn FnMut(bool, u32) + Send + Sync>,
}

impl std::fmt::Debug for ViewportListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewportListener")
            .field("widget", &self.widget)
            .field("viewport", &self.viewport)
            .field("visible", &self.visible)
            .finish_non_exhaustive()
    }
}

impl ViewportListener {
    pub fn new(
        widget: impl WidgetPosition + 'static,
        viewport: Option<Viewport>,
        callback: impl FnMut(bool, u32) + Send + Sync + 'static,
    ) -> Self {
        let mut this = Self {
            widget: Box::new(widget),
            viewport,
            visible: false,
            dist: 0,
            callback: Box::new(callback),
        };

        this.init();
        this
    }

    fn is_visible(&self) -> (bool, u32) {
        if let Some((visible, dist)) = self
            .viewport
            .as_ref()
            .map(|x| x.is_widget_visible(&*self.widget))
        {
            (visible, dist)
        } else {
            (true, 0)
        }
    }

    fn init(&mut self) {
        let (visible, dist) = self.is_visible();
        self.visible = visible;
        self.dist = dist;
        (self.callback)(visible, dist);
    }

    pub fn check(&mut self) {
        let (visible, dist) = self.is_visible();

        if visible != self.visible || dist != self.dist {
            self.visible = visible;
            self.dist = dist;
            (self.callback)(visible, dist);
        }
    }
}
