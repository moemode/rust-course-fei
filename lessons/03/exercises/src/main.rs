//! You can use this file for experiments.

use week03::merge_slices::merge_slices;

fn main() {
    let left = vec!["apple", "banana", "cherry"];
    let right = vec!["blueberry", "date", "elderberry"];

    let merged = merge_slices(&left, &right);
    println!("Merged result: {:?}", merged);
}
