//! A small learned decision head over embeddings (decision 0019).
//!
//! Embedding similarity captures what a request is ABOUT, not what it asks
//! FOR: "remember my accountant's name is Laura" and "what is my accountant's
//! name?" sit next to each other. A multinomial logistic regression trained
//! on a few labeled example requests per option learns the directions that
//! separate them. It is trained at prewarm, on-device, over whichever
//! embedder is installed (so switching the embedder retrains it and nothing
//! is shared with another model's vectors), deterministic, and its output is
//! a probability - the calibration bars apply to probabilities, not to raw
//! cosine. It only RECOMMENDS; nothing it says authorizes anything.

#[derive(Debug, Clone)]
pub struct LinearHead {
    classes: usize,
    dim: usize,
    /// `classes * dim` weights, row-major by class.
    w: Vec<f32>,
    b: Vec<f32>,
}

fn l2_normalized(v: &[f32]) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    v.iter().map(|x| x / n).collect()
}

fn softmax(z: &[f32]) -> Vec<f32> {
    let m = z.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let e: Vec<f32> = z.iter().map(|x| (x - m).exp()).collect();
    let s: f32 = e.iter().sum();
    e.into_iter().map(|x| x / s).collect()
}

impl LinearHead {
    /// Full-batch gradient descent with L2 regularisation. Inputs are
    /// L2-normalised; training is deterministic (zero init, fixed schedule).
    pub fn train(xs: &[Vec<f32>], ys: &[usize], classes: usize) -> Option<LinearHead> {
        let dim = xs.first()?.len();
        if xs.len() != ys.len()
            || xs.iter().any(|x| x.len() != dim)
            || ys.iter().any(|y| *y >= classes)
        {
            return None;
        }
        let xs: Vec<Vec<f32>> = xs.iter().map(|x| l2_normalized(x)).collect();
        let n = xs.len() as f32;
        let mut head = LinearHead {
            classes,
            dim,
            w: vec![0.0; classes * dim],
            b: vec![0.0; classes],
        };
        let (lr, l2, iters) = (6.0f32, 2e-4f32, 500);
        for _ in 0..iters {
            let mut gw = vec![0.0f32; classes * dim];
            let mut gb = vec![0.0f32; classes];
            for (x, &y) in xs.iter().zip(ys) {
                let p = softmax(&head.logits(x));
                for c in 0..classes {
                    let g = p[c] - if c == y { 1.0 } else { 0.0 };
                    gb[c] += g;
                    let row = &mut gw[c * dim..(c + 1) * dim];
                    for (r, xi) in row.iter_mut().zip(x) {
                        *r += g * xi;
                    }
                }
            }
            for (w, g) in head.w.iter_mut().zip(&gw) {
                *w -= lr * (g / n + l2 * *w);
            }
            for (b, g) in head.b.iter_mut().zip(&gb) {
                *b -= lr * g / n;
            }
        }
        head.w.iter().all(|x| x.is_finite()).then_some(head)
    }

    fn logits(&self, x_normalized: &[f32]) -> Vec<f32> {
        (0..self.classes)
            .map(|c| {
                self.w[c * self.dim..(c + 1) * self.dim]
                    .iter()
                    .zip(x_normalized)
                    .map(|(w, x)| w * x)
                    .sum::<f32>()
                    + self.b[c]
            })
            .collect()
    }

    /// Class probabilities for one (unnormalised) embedding.
    pub fn predict(&self, v: &[f32]) -> Option<Vec<f32>> {
        (v.len() == self.dim).then(|| softmax(&self.logits(&l2_normalized(v))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_directions_that_nearest_neighbours_confuse() {
        // Two classes that share a topic axis (dim 0) and differ on dim 1:
        // cosine to class means is near-tied, the head separates them.
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for i in 0..8 {
            let t = 1.0 + 0.01 * i as f32;
            xs.push(vec![t, 0.30, 0.0]);
            ys.push(0);
            xs.push(vec![t, -0.30, 0.0]);
            ys.push(1);
        }
        let h = LinearHead::train(&xs, &ys, 2).unwrap();
        let p0 = h.predict(&[1.0, 0.3, 0.0]).unwrap();
        let p1 = h.predict(&[1.0, -0.3, 0.0]).unwrap();
        assert!(p0[0] > 0.9 && p1[1] > 0.9, "{p0:?} {p1:?}");
        assert!((p0.iter().sum::<f32>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn training_is_deterministic_and_refuses_bad_input() {
        let xs = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let a = LinearHead::train(&xs, &[0, 1], 2).unwrap();
        let b = LinearHead::train(&xs, &[0, 1], 2).unwrap();
        assert_eq!(a.w, b.w);
        assert!(LinearHead::train(&xs, &[0, 5], 2).is_none());
        assert!(LinearHead::train(&[], &[], 2).is_none());
        assert!(a.predict(&[1.0]).is_none());
    }
}
