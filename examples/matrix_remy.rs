use clap::Parser;
use halo2_base::gates::{GateChip, GateInstructions, RangeChip, RangeInstructions};
use halo2_base::utils::{BigPrimeField, ScalarField};
use halo2_base::AssignedValue;
#[allow(unused_imports)]
use halo2_base::{
    Context,
    QuantumCell::{Constant, Existing, Witness},
};
use halo2_scaffold::gadget::fixed_point::{FixedPointChip, FixedPointInstructions};
use halo2_scaffold::scaffold::cmd::Cli;
use halo2_scaffold::scaffold::run;
use serde::{Deserialize, Serialize};
use std::env::{set_var, var};
use poseidon::PoseidonChip;

const T: usize = 3;
const RATE: usize = 2;
const R_F: usize = 8;
const R_P: usize = 57;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CircuitInput {
    pub d: Vec<f64>,
    pub m: Vec<Vec<f64>>,
    pub u: Vec<Vec<f64>>,
    pub v: Vec<Vec<f64>>,
     // field element, but easier to deserialize as a string
}

fn some_algorithm_in_zk<F: ScalarField>(
    ctx: &mut Context<F>,
    input: CircuitInput,
    make_public: &mut Vec<AssignedValue<F>>,
) where
    F: BigPrimeField,
{
   
   
    
    // `Context` can roughly be thought of as a single-threaded execution trace of a program we want to ZK prove. We do some post-processing on `Context` to optimally divide the execution trace into multiple columns in a PLONKish arithmetization
    // More advanced usage with multi-threaded witness generation is possible, but we do not explain it here

    // lookup bits must agree with the size of the lookup table, which is specified by an environmental variable
    let lookup_bits =
        var("LOOKUP_BITS").unwrap_or_else(|_| panic!("LOOKUP_BITS not set")).parse().unwrap();
    const PRECISION_BITS: u32 = 32;
    // fixed-point exp arithmetic
    let fpchip = FixedPointChip::<F, PRECISION_BITS>::default(lookup_bits);

    const N: usize = 5;
    const M: usize = 4;
    let mut matrix: Vec<Vec<f64>> = Vec::new();
    for i in 0..N {
        matrix.push(Vec::<f64>::new());
        for j in 0..M {
            matrix[i].push((i as f64) + (j as f64) / 10.0);
        }
    }
    println!("{:?}", matrix);

    let zkmatrix: ZkMatrix<F, PRECISION_BITS> = ZkMatrix::new(ctx, &fpchip, matrix);

    zkmatrix.print(&fpchip);

    // Lets try to implement Poseidon below
    
    let x = F::from(3456789);
    let y = F::from(221226723898766);

    let x = ctx.load_witness(x);
    let y = ctx.load_witness(y);


    let gate = GateChip::<F>::default();
    let mut poseidon = PoseidonChip::<F, T, RATE>::new(ctx, R_F, R_P).unwrap();
    poseidon.update(&[x, y]);
    let hash = poseidon.squeeze(ctx, &gate).unwrap();
    make_public.push(hash);
    println!("x: {:?}, y: {:?}, poseidon(x): {:?}", x.value(), y.value(), hash.value());

    // seems to be working ! Lets now generate a vector of o and 1 from the output value of the hash function
    
    let vec_bits = gate.num_to_bits(ctx, hash, 254);

    let mut challenge_vec: Vec<AssignedValue<F>> = Vec::new();

    for r in vec_bits.iter() {
        challenge_vec.push(*r);
    }

    // seems to be working, lets now build d vector by succesively hashing x and y

    let nums =20;

    for _i in 0..nums {
        poseidon.update(&[hash, y]);
        let hash = poseidon.squeeze(ctx, &gate).unwrap();
        let vec_bits = gate.num_to_bits(ctx, hash, 254);
        for r in vec_bits.iter() {
            challenge_vec.push(*r);
        }

    }

    let challenge_vec: ZkVector<F, PRECISION_BITS> = ZkVector{v: challenge_vec};

    

    //println!("vector of bits: {:?}", vec_bits);



    // SVD check below

    let m = input.m;
    let u = input.u;
    let v = input.v;

    let mq : ZkMatrix<F, PRECISION_BITS> = ZkMatrix::new(ctx, &fpchip, m);
    let uq : ZkMatrix<F, PRECISION_BITS> = ZkMatrix::new(ctx, &fpchip, u);
    let vq : ZkMatrix<F, PRECISION_BITS> = ZkMatrix::new(ctx, &fpchip, v);

    let d = input.d;
    let dq : ZkVector<F, PRECISION_BITS> = ZkVector::new(ctx, &fpchip, d);

    ZkMatrix::verify_svd(ctx, &fpchip, &mq, &uq, &dq, &vq, 0.000001);





    // mat1q.print(&fpchip);



    




    // first we load a number `x` into as system, as a "witness"
    // by default, all numbers in the system are private
    // we can make it public like so:
}

pub struct ZkVector<F: BigPrimeField, const PRECISION_BITS: u32> {
    v: Vec<AssignedValue<F>>,
}

impl<F: BigPrimeField, const PRECISION_BITS: u32> ZkVector<F, PRECISION_BITS> {
    pub fn new(
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        v: Vec<f64>,
    ) -> Self {
        let mut zk_v: Vec<AssignedValue<F>> = Vec::new();
        for elem in v {
            let elem = fpchip.quantization(elem);
            zk_v.push(ctx.load_witness(elem));
        }
        return Self { v: zk_v };
    }

    pub fn size(&self) -> usize {
        return self.v.len();
    }

    pub fn dequantize(&self, fpchip: &FixedPointChip<F, PRECISION_BITS>) -> Vec<f64> {
        let mut dq_v: Vec<f64> = Vec::new();
        for elem in &self.v {
            dq_v.push(fpchip.dequantization(*elem.value()));
        }
        return dq_v;
    }

    pub fn print(&self, fpchip: &FixedPointChip<F, PRECISION_BITS>) {
        let dq_v = self.dequantize(fpchip);
        print!("{:?}", dq_v);
    }

    /// Calculates and constrains the inner product of this vector with vector x
    /// Outputs the inner product
    /// Order doesn't matter because we are only dealing with real numbers
    pub fn inner_product(
        &self,
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        x: &Vec<AssignedValue<F>>,
    ) -> AssignedValue<F> {
        assert!(self.size() == x.len());
        let mut res = fpchip.qadd(ctx, Constant(F::zero()), Constant(F::zero()));
        for i in 0..self.size() {
            let ai_bi = fpchip.qmul(ctx, self.v[i], x[i]);
            res = fpchip.qadd(ctx, res, ai_bi);
        }

        return res;
    }

    pub fn norm(
        &self,
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
    ) -> AssignedValue<F> {
        return self.inner_product(ctx, fpchip, &self.v);
    }
    
    pub fn dist(
        &self,
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        x: &Vec<AssignedValue<F>>,
    ) -> AssignedValue<F> {
        assert_eq!(self.size(), x.len());
        let mut diff: Vec<AssignedValue<F>> = Vec::new();
        for (r, s) in self.v.iter().zip(x.iter()) {
            diff.push(fpchip.qsub(ctx, *r, *s));
        }
        let diff = Self { v: diff };
        return diff.norm(ctx, fpchip);
    }
    /// Multiplies this vector by matrix `a` in the zk-circuit and returns the constrained output `a.v`
    pub fn mul(
        &self,
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        a: &ZkMatrix<F, PRECISION_BITS>,
    ) -> Self {
        assert_eq!(a.num_col, self.size());
        let mut y: Vec<AssignedValue<F>> = Vec::new();
        for row in &a.matrix {
            y.push(self.inner_product(ctx, fpchip, row));
        }
        return Self { v: y };
    }
}

pub struct ZkMatrix<F: BigPrimeField, const PRECISION_BITS: u32> {
    matrix: Vec<Vec<AssignedValue<F>>>,
    num_rows: usize,
    num_col: usize,
}
impl<F: BigPrimeField, const PRECISION_BITS: u32> ZkMatrix<F, PRECISION_BITS> {
    pub fn new(
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        matrix: Vec<Vec<f64>>,
    ) -> Self {
        let mut zkmatrix: Vec<Vec<AssignedValue<F>>> = Vec::new();
        let num_rows = matrix.len();
        let num_col = matrix[0].len();
        for row in &matrix {
            assert!(row.len() == num_col);
        }
        for i in 0..num_rows {
            let mut new_row: Vec<AssignedValue<F>> = Vec::new();
            for j in 0..num_col {
                let elem = matrix[i][j];
                let elem = fpchip.quantization(elem);
                new_row.push(ctx.load_witness(elem));
            }
            zkmatrix.push(new_row);
        }
        return Self { matrix: zkmatrix, num_rows: num_rows, num_col: num_col };
    }
    pub fn dequantize(&self, fpchip: &FixedPointChip<F, PRECISION_BITS>) -> Vec<Vec<f64>> {
        let mut dq_matrix: Vec<Vec<f64>> = Vec::new();
        for i in 0..self.num_rows {
            dq_matrix.push(Vec::<f64>::new());
            for j in 0..self.num_col {
                let elem = self.matrix[i][j];
                dq_matrix[i].push(fpchip.dequantization(*elem.value()));
            }
        }
        return dq_matrix;
    }
    pub fn print(&self, fpchip: &FixedPointChip<F, PRECISION_BITS>) {
        print!("[\n");
        for i in 0..self.num_rows {
            for j in 0..self.num_col {
                let elem = self.matrix[i][j];
                let elem = fpchip.dequantization(*elem.value());
                print!("{}, ", elem);
            }
            print!("\n");
        }
        print!("]");
    }

   


    /// Verifies that matrices `a`, `b`, and `c` satisfy `c = a*b`
    pub fn verify_mul(
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        a: &Self,
        b: &Self,
        c: &Self,
        k_iter: usize,
        cha_vec: Vec<AssignedValue<F>>,
    ) {
        assert_eq!(a.num_col, b.num_rows);
        assert_eq!(c.num_rows, a.num_rows);
        assert_eq!(c.num_col, b.num_col);

        for i in 0..k_iter{




        }

        // generate a random vector
        let mut v: Vec<f64> = Vec::new();
        for i in 0..b.num_col {
            v.push(i as f64);
        }
        let v = ZkVector::new(ctx, fpchip, v);

        let c_times_v = v.mul(ctx, fpchip, c);
        let b_times_v = v.mul(ctx, fpchip, b);
        let ab_times_v = b_times_v.mul(ctx, fpchip, a);

        // ensure that norm between cv and abv is small
    }
        
        
        
        // Computes by the basic method the product of two matrices a and b
    pub fn matrix_mul(
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        a: &Self,
        b: &Self,
    )-> Self {

        assert_eq!(a.num_col, b.num_rows);

        let mut c: Vec<Vec<AssignedValue<F>>> = Vec::new();

        for i in 0..a.num_rows{
            let mut new_row: Vec<AssignedValue<F>> = Vec::new();
            for j in 0..b.num_col{
                let mut new_ele = ctx.load_witness(F::from(0));
                for k in 0..a.num_col{
                    let prod = fpchip.qmul(ctx, a.matrix[i][k], b.matrix[k][j]);
                    new_ele = fpchip.qadd(ctx, new_ele, prod);
                }
                new_row.push(new_ele);
            }
            c.push(new_row);
        }
        return Self { matrix: c, num_rows: a.num_rows, num_col: b.num_col};
        
    }

        // Function for multiplying two matrices a and d were d is diagonal, d is given as a vector

    pub fn matrix_mul_diag(
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        a: &Self,
        d: &ZkVector<F, PRECISION_BITS>,
    )-> Self {

        let l = ZkVector::size(d);

        assert_eq!(a.num_col, l);

        let mut c: Vec<Vec<AssignedValue<F>>> = Vec::new();

        for i in 0..a.num_rows{
            let mut new_row: Vec<AssignedValue<F>> = Vec::new();
            for j in 0..a.num_col{
                let prod = fpchip.qmul(ctx, a.matrix[i][j], d.v[j]);
                new_row.push(prod);
            }
            c.push(new_row);
        }
        return Self { matrix: c, num_rows: a.num_rows, num_col: a.num_col};
        
    }

        // Verify the product of the two matrices by the naive method up to a small error

    pub fn verify_mul_simple(
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        a: &Self,
        b: &Self,
        c: &Self,
        delta: f64,
    ) {
        let range = fpchip.range_gate();

        assert_eq!(a.num_col, b.num_rows);
        assert_eq!(c.num_rows, a.num_rows);
        assert_eq!(c.num_col, b.num_col);
        
        let c_check = ZkMatrix::matrix_mul(ctx, fpchip, a, b);

        let error = ctx.load_witness(fpchip.quantization(delta));

        for i in 0..c.num_rows{
            for j in 0..c.num_col{
                let ele_dif = fpchip.qsub(ctx, c.matrix[i][j], c_check.matrix[i][j]);
                let ele_abs = fpchip.qabs(ctx, ele_dif);
                range.check_less_than(ctx, ele_abs, error, 64);
            }
        }
    }

        // define a function for matrix transpose

    pub fn transpose_matrix(
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        a: &Self,
    )-> Self {

        let mut a_trans: Vec<Vec<AssignedValue<F>>> = Vec::new();

        for i in 0..a.num_col{
            let mut new_row: Vec<AssignedValue<F>> = Vec::new();
            for j in 0..a.num_rows{
                new_row.push(a.matrix[j][i]);
            }
            a_trans.push(new_row);
        }
        return Self { matrix: a_trans, num_rows: a.num_col, num_col: a.num_rows};
    }

        // Verify that a matrix is orthogonal using the two previous functions

    pub fn check_is_ortho(
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        a: &Self,
        delta: f64
    ) {
        assert_eq!(a.num_col, a.num_rows);

        let mut id_mat_float: Vec<Vec<f64>> = Vec::new();

        for i in 0..a.num_rows{
            let mut new_row: Vec<f64> = Vec::new();
            for j in 0..a.num_col{
                if i == j {
                    new_row.push(1.0);
                }
                else {
                    new_row.push(0.0);
                }
            }
            id_mat_float.push(new_row);
        }
        
        let id_mat = ZkMatrix::new(ctx, fpchip, id_mat_float);

        let a_trans = ZkMatrix::transpose_matrix(ctx, fpchip, a);

        ZkMatrix::verify_mul_simple(ctx, fpchip, a, &a_trans, &id_mat, delta);

    }

    // Check that a matrix is upper-triangular

    pub fn check_is_uppertri(
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        a: &Self,
    ) {
        let gate = GateChip::<F>::default();

        for i in 0..a.num_rows{
            for j in 0..a.num_col{
                if i > j {
                    gate.is_zero(ctx, a.matrix[i][j]);
                }
            }
        }
    }

    // Check that a matrix is diagomal

    pub fn check_is_diag(
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        a: &Self,
    ) {
        let gate = GateChip::<F>::default();

        for i in 0..a.num_rows{
            for j in 0..a.num_col{
                if i != j {
                    gate.is_zero(ctx, a.matrix[i][j]);
                }
            }
        }
    }

    // Check SVD decomposition of a matrix, namely m = u d v, u and v are ortho, d diag

    pub fn verify_svd(
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        m: &Self,
        u: &Self,
        d: &ZkVector<F,PRECISION_BITS>,
        v: &Self,
        delta: f64,
    ) {
        
        ZkMatrix::check_is_ortho(ctx, fpchip, u, delta);
        ZkMatrix::check_is_ortho(ctx, fpchip, v, delta);

        let prod = ZkMatrix::matrix_mul_diag(ctx, fpchip, u, d);

        ZkMatrix::verify_mul_simple(ctx, fpchip, &prod, v, m, delta);
    }

    // Check QR decomposition, m= qr, q orthogonal, r upper triangular

    pub fn verify_qr(
        ctx: &mut Context<F>,
        fpchip: &FixedPointChip<F, PRECISION_BITS>,
        m: &Self,
        q: &Self,
        r: &Self,
        delta: f64,
    ) {
        
        ZkMatrix::check_is_ortho(ctx, fpchip, q, delta);
        ZkMatrix::check_is_uppertri(ctx, fpchip, r);
        ZkMatrix::verify_mul_simple(ctx, fpchip, q, r, m, delta);
    }


}

fn main() {
    env_logger::init();

    let args = Cli::parse();

    // run different zk commands based on the command line arguments
    run(some_algorithm_in_zk, args);
}
