//! ASCII DXF entity writer shared by board and laser-path exporters.

pub(crate) const MAX_DXF_ENTITIES: usize = 20_000;

pub(crate) struct DxfWriter {
    pub entities: String,
    pub count: usize,
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl DxfWriter {
    pub fn new() -> Self {
        Self {
            entities: String::new(),
            count: 0,
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
        }
    }

    pub(crate) fn follow(&mut self, x: f64, y: f64) {
        if self.count == 0 {
            self.min_x = x;
            self.min_y = y;
            self.max_x = x;
            self.max_y = y;
        } else {
            self.min_x = self.min_x.min(x);
            self.min_y = self.min_y.min(y);
            self.max_x = self.max_x.max(x);
            self.max_y = self.max_y.max(y);
        }
    }

    pub fn keep(&mut self, layer: &str, body: String) -> Result<(), String> {
        self.count += 1;
        if self.count > MAX_DXF_ENTITIES {
            return Err("DXF export exceeds the entity limit".into());
        }
        self.entities.push_str("0\nENTITY\n8\n");
        self.entities.push_str(layer);
        self.entities.push('\n');
        self.entities.push_str(&body);
        Ok(())
    }

    pub fn line(&mut self, layer: &str, a: (f64, f64), b: (f64, f64)) -> Result<(), String> {
        self.follow(a.0, a.1);
        self.follow(b.0, b.1);
        self.keep(
            layer,
            format!(
                "0\nLINE\n10\n{:.3}\n20\n{:.3}\n11\n{:.3}\n21\n{:.3}\n",
                a.0, a.1, b.0, b.1
            ),
        )
    }

    pub fn arc(
        &mut self,
        layer: &str,
        c: (f64, f64),
        r: f64,
        from_deg: f64,
        to_deg: f64,
    ) -> Result<(), String> {
        self.follow(c.0 - r, c.1 - r);
        self.follow(c.0 + r, c.1 + r);
        self.keep(
            layer,
            format!(
                "0\nARC\n10\n{:.3}\n20\n{:.3}\n40\n{:.3}\n50\n{:.6}\n51\n{:.6}\n",
                c.0, c.1, r, from_deg, to_deg
            ),
        )
    }

    pub fn circle(&mut self, layer: &str, x: f64, y: f64, r: f64) -> Result<(), String> {
        self.follow(x - r, y - r);
        self.follow(x + r, y + r);
        self.keep(
            layer,
            format!("0\nCIRCLE\n10\n{x:.3}\n20\n{y:.3}\n40\n{r:.3}\n"),
        )
    }

    pub fn polyline(
        &mut self,
        layer: &str,
        points: &[(f64, f64)],
        closed: bool,
    ) -> Result<(), String> {
        let mut body = format!(
            "0\nLWPOLYLINE\n90\n{}\n70\n{}\n",
            points.len(),
            u8::from(closed)
        );
        for (x, y) in points {
            self.follow(*x, *y);
            body.push_str(&format!("10\n{x:.3}\n20\n{y:.3}\n"));
        }
        self.keep(layer, body)
    }

    pub fn text(
        &mut self,
        layer: &str,
        x: f64,
        y: f64,
        height: f64,
        rotation: f64,
        value: &str,
    ) -> Result<(), String> {
        self.follow(x, y);
        self.keep(
            layer,
            format!(
                "0\nTEXT\n10\n{x:.3}\n20\n{y:.3}\n40\n{height:.3}\n50\n{rotation:.1}\n1\n{value}\n"
            ),
        )
    }
}

pub fn dxf_document(w: &DxfWriter) -> String {
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$INSUNITS\n70\n4\n9\n$EXTMIN\n10\n{:.3}\n20\n{:.3}\n9\n$EXTMAX\n10\n{:.3}\n20\n{:.3}\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        w.min_x, w.min_y, w.max_x, w.max_y, w.entities
    )
}
