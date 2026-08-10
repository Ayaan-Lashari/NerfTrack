fn main() {
    if nerftrack_lib::updater::run_update_helper_if_requested() {
        return;
    }
    nerftrack_lib::run();
}
