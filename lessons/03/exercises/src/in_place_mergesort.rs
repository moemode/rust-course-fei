pub fn merge_sort<T: Ord>(arr: &mut [T]) {
    let n = arr.len();
    let mut width = 1;
    while width < n {
        let mut i = 0;
        while i < n {
            let left = i;
            let mid = i + width;
            let right = if i + 2 * width > n { n } else { i + 2 * width };
            in_place_merge(arr, left, mid, right);
            i += 2 * width;
        }
        width *= 2;
    }
}

fn in_place_merge<T: Ord>(arr: &mut [T], left: usize, mid: usize, right: usize) {
    let mut i = left;
    let mut j = mid;
    while i < j && j < right {
        if arr[i] > arr[j] {
            j += 1;
            arr[i..j].rotate_right(1);
        }
        i += 1;
    }
}

fn in_place_merge_old<T: Ord>(arr: &mut [T], left: usize, mid: usize, right: usize) {
    let mut i = left;
    let mut j = mid;
    while i < j && j < right {
        if arr[i] <= arr[j] {
            i += 1;
        } else {
            let mut k = j;
            while k > i {
                arr.swap(k, k - 1);
                k -= 1;
            }
            i += 1;
            j += 1;
        }
    }
}
