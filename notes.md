T is a superset of both &T and &mut T
&T and &mut T are disjoint sets

&'static T is an immutable reference to some T that can be safely held indefinitely long, including up until the end of the program. This is only possible if T itself is immutable and does not move after the reference was created
T: 'static includes all &'static T however it also includes all owned types, like String, Vec, etc.
if T: 'static then T can be a borrowed type with a 'static lifetime or an owned type
