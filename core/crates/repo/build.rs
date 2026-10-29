// sqlx::migrate! embeds the migrations at compile time: rebuild when one is added or changed
// (cargo doesn't otherwise notice a new file in that folder).
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
