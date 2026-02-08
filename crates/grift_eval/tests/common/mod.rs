use grift_eval::*;

#[allow(dead_code)]
pub fn eval_to_num<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> isize {
    let result = eval.eval_str(input).unwrap();
    lisp.get(result).unwrap().as_number().unwrap()
}

#[allow(dead_code)]
pub fn eval_is_true<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> bool {
    let result = eval.eval_str(input).unwrap();
    lisp.get(result).unwrap().is_true()
}

#[allow(dead_code)]
pub fn eval_is_false<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> bool {
    let result = eval.eval_str(input).unwrap();
    lisp.get(result).unwrap().is_false()
}

#[allow(dead_code)]
pub fn eval_to_string<const N: usize>(lisp: &Lisp<N>, eval: &mut Evaluator<N>, input: &str) -> String {
    let result = eval.eval_str(input).unwrap();
    format!("{}", lisp.display(result))
}
