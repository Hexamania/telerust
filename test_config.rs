use directories::ProjectDirs;

fn main() {
    let proj_dirs = ProjectDirs::from("com", "telerust", "telerust")
        .expect("Could not determine config directory");
    
    println!("config_dir: {:?}", proj_dirs.config_dir());
    println!("data_dir: {:?}", proj_dirs.data_dir());
    println!("cache_dir: {:?}", proj_dirs.cache_dir());
    
    let config_file = proj_dirs.config_dir().join("config.toml");
    println!("config_file: {:?}", config_file);
    println!("exists: {:?}", config_file.exists());
}
