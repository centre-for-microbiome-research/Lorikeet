use ndarray::{Array2, Array1, Axis, ArrayView, Ix1, prelude::*};
use ndarray_linalg::{SVD, convert::*, diagonal::*, Norm};
use rayon::prelude::*;
use std::sync::{Arc, Mutex};
use std::process;

#[derive(Debug, Clone, Copy)]
pub enum Seed {
    Nndsvd {
        rank: usize,
    },
    None,
}

impl Seed {
    pub fn new_nndsvd(rank: usize, v: &Array2<f32>) -> Seed {
        Seed::Nndsvd {
            rank,
        }
    }
}

pub trait SeedFunctions {
    fn initialize(&self, v: &Array2<f32>) -> (Array2<f32>, Array2<f32>);
}

impl SeedFunctions for Seed {
    fn initialize(&self, v: &Array2<f32>) -> (Array2<f32>, Array2<f32>) {
        match self {
            Seed::Nndsvd {
                rank,
            } => {
                let (u, s, e)
                    = v.svd(true, true).unwrap();
                let e = e.unwrap();
                let e = e.t();
                let u = u.unwrap();

                let mut w = Array2::zeros((v.shape()[0], *rank));
                let mut h = Array2::zeros((*rank, v.shape()[1]));

                // choose the first singular triplet to be nonnegative
                let s = s.into_diag();
                debug!("S: {:?}", s);
                w.slice_mut(s![.., 0]).assign(
                    &(s[0].powf(1. / 2.) * u.slice(s![.., 0]).mapv(|x| x.abs())));
                h.slice_mut(s![0, ..]).assign(
                    &(s[0].powf(1. / 2.) * e.slice(s![.., 0]).t().mapv(|x| x.abs())));

                // generate mutex guards around w and h
                let w_guard = Arc::new(Mutex::new(w.clone()));
                let h_guard = Arc::new(Mutex::new(h.clone()));

                // Update other factors based on associated svd factor
                (1..*rank).into_par_iter().for_each(|i|{
                    let uu = u.slice(s![.., i]).to_owned();
                    let vv = e.slice(s![.., i]).to_owned();
                    let mut uup = pos(&uu);
                    let mut uun = neg(&uu);
                    let vvp = pos(&vv);
                    let vvn = neg(&vv);
                    let n_uup = uup.norm();
                    let n_uun = uun.norm();
                    let n_vvp = vvp.norm();
                    let n_vvn = vvn.norm();
                    let termp = n_uup * n_vvp;
                    let termn = n_uun * n_vvn;

                    if termp >= termn {
                        let mut w_guard = w_guard.lock().unwrap();
                        let mut h_guard = h_guard.lock().unwrap();

                        uup.par_mapv_inplace(|x| x * n_uup);
                        let mut vvp_t = vvp.t().to_owned();
                        vvp_t.par_mapv_inplace(|x| x * n_vvp);

                        w_guard.slice_mut(s![.., i]).assign(
                            &((s[i] * termp).powf(1. / 2.) / (uup)));
                        h_guard.slice_mut(s![i, ..]).assign(
                            &((s[i] * termp).powf(1. / 2.) / (vvp_t)));;
                    } else {
                        let mut w_guard = w_guard.lock().unwrap();
                        let mut h_guard = h_guard.lock().unwrap();

                        uun.par_mapv_inplace(|x| x * n_uun);
                        let mut vvn_t = vvn.t().to_owned();
                        vvn_t.par_mapv_inplace(|x| x * n_vvn);

                        w_guard.slice_mut(s![.., i]).assign(
                            &((s[i] * termn).powf(1. / 2.) / (uun)));
                        h_guard.slice_mut(s![i, ..]).assign(
                            &((s[i] * termn).powf(1. / 2.) / (vvn_t)));;
                    }
                });
                let mut w_guard = w_guard.lock().unwrap();
                let mut h_guard = h_guard.lock().unwrap();

                w_guard.par_mapv_inplace(|x|{
                    if x < 1f32.exp().powf(-11.) {
                        0.
                    } else {
                        x
                    }
                });

                h_guard.par_mapv_inplace(|x|{
                    if x < 1f32.exp().powf(-11.) {
                        0.
                    } else {
                        x
                    }
                });

                let w = w_guard.clone();
                let h = h_guard.clone();

                debug!("Threshold {}", 1f32.exp().powf(-11.));
                return (w, h)

            },
            Seed::None => process::exit(1)
        }
    }
}

fn pos(matrix: &Array1<f32>) -> Array1<f32> {
    let mut pos_mat = matrix.to_owned();
    pos_mat.par_mapv_inplace(|x| {
        if x >= 0. {
            1.
        } else {
            0.
        }
    });
    pos_mat * matrix
}

fn neg(matrix: &Array1<f32>) -> Array1<f32> {
    let mut neg_mat = matrix.to_owned();
    neg_mat.par_mapv_inplace(|x| {
        if x < 0. {
            1.
        } else {
            0.
        }
    });
    neg_mat * -matrix

//        matrix.mapv(|x| {
//        if x != 0. {
//            -x
//        } else {
//            x
//        }
//    })
}
