pub fn factorial(n: u64) -> u64 {
    (1..=n).product()
}

pub fn find_largest_helper<A, T>(collection: A) -> Option<T>
where
    A: AsRef<[T]>,
    T: Ord + Clone,
{
    let arr = collection.as_ref();
    if arr.is_empty() {
        return None;
    }
    let mut largest = &arr[0];
    for item in &arr[1..] {
        if item > largest {
            largest = item;
        }
    }
    Some(largest.clone())
}

pub fn find_largest<A, T>(collection: A) -> T
where
    A: AsRef<[T]>,
    T: Ord + Clone,
{
    find_largest_helper(collection).unwrap()
}
