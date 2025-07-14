use macroquad::prelude::*;

// Lattice Boltzmann parameters
const NX: usize = 200;
const NY: usize = 100;
const Q: usize = 9;
const W: [f32; Q] = [4.0/9.0, 1.0/9.0, 1.0/9.0, 1.0/9.0, 1.0/9.0,
                     1.0/36.0, 1.0/36.0, 1.0/36.0, 1.0/36.0];
const CX: [i32; Q] = [0, 1, 0, -1, 0, 1, -1, -1, 1];
const CY: [i32; Q] = [0, 0, 1, 0, -1, 1, 1, -1, -1];
const OMEGA: f32 = 1.0; // relaxation parameter (1/tau)

struct LBM {
    f: Vec<f32>,
    f_tmp: Vec<f32>,
    rho: Vec<f32>,
    ux: Vec<f32>,
    uy: Vec<f32>,
}

impl LBM {
    fn new() -> Self {
        let size = NX * NY;
        let f = vec![0.0; size * Q];
        let f_tmp = f.clone();
        let rho = vec![1.0; size];
        let ux = vec![0.0; size];
        let uy = vec![0.0; size];
        let mut lbm = LBM { f, f_tmp, rho, ux, uy };
        lbm.initialize();
        lbm
    }

    fn index(&self, x: usize, y: usize, i: usize) -> usize {
        (y * NX + x) * Q + i
    }

    fn initialize(&mut self) {
        for y in 0..NY {
            for x in 0..NX {
                let idx = y * NX + x;
                for i in 0..Q {
                    self.f[idx * Q + i] = W[i];
                }
            }
        }
    }

    fn equilibrium(&self, rho: f32, ux: f32, uy: f32, i: usize) -> f32 {
        let cu = CX[i] as f32 * ux + CY[i] as f32 * uy;
        let u2 = ux * ux + uy * uy;
        W[i] * rho * (1.0 + 3.0 * cu + 4.5 * cu * cu - 1.5 * u2)
    }

    fn collide(&mut self) {
        for y in 0..NY {
            for x in 0..NX {
                let idx = y * NX + x;
                let mut rho = 0.0;
                let mut ux = 0.0;
                let mut uy = 0.0;
                for i in 0..Q {
                    let fi = self.f[idx * Q + i];
                    rho += fi;
                    ux += fi * CX[i] as f32;
                    uy += fi * CY[i] as f32;
                }
                ux /= rho;
                uy /= rho;
                self.rho[idx] = rho;
                self.ux[idx] = ux;
                self.uy[idx] = uy;
                for i in 0..Q {
                    let feq = self.equilibrium(rho, ux, uy, i);
                    let fi = &mut self.f[idx * Q + i];
                    *fi += OMEGA * (feq - *fi);
                }
            }
        }
    }

    fn stream(&mut self) {
        // Copy to tmp
        self.f_tmp.copy_from_slice(&self.f);
        for y in 0..NY {
            for x in 0..NX {
                for i in 0..Q {
                    let x_src = (x as i32 - CX[i] + NX as i32) as usize % NX;
                    let y_src = (y as i32 - CY[i] + NY as i32) as usize % NY;
                    let dst = self.index(x, y, i);
                    let src = self.index(x_src, y_src, i);
                    self.f[dst] = self.f_tmp[src];
                }
            }
        }
    }

    fn bounce_back(&mut self) {
        // Simple bounce-back at top and bottom walls
        for x in 0..NX {
            // bottom y=0
            let y = 0;
            let idx = y * NX + x;
            for (i, &opp) in [4,5,6,1,2,3,8,7,0].iter().enumerate() {
                let dst = idx * Q + i;
                let src = idx * Q + opp;
                self.f[dst] = self.f_tmp[src];
            }
            // top y=NY-1
            let y = NY-1;
            let idx = y * NX + x;
            for (i, &opp) in [4,5,6,1,2,3,8,7,0].iter().enumerate() {
                let dst = idx * Q + i;
                let src = idx * Q + opp;
                self.f[dst] = self.f_tmp[src];
            }
        }
    }
}

#[macroquad::main("Lattice Boltzmann Fluid Simulation")]
async fn main() {
    let mut lbm = LBM::new();
    loop {
        // update
        lbm.collide();
        lbm.stream();
        lbm.bounce_back();

        // render density field
        for y in 0..NY {
            for x in 0..NX {
                let idx = y * NX + x;
                let rho = lbm.rho[idx];
                let c = (rho * 255.0) as u8;
                draw_rectangle(
                    x as f32, y as f32, 1.0, 1.0,
                    Color::from_rgba(c, c, c, 255)
                );
            }
        }
        next_frame().await;
    }
}