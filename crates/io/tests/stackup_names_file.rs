//! A project file saved before copper layers were named by stackup still opens
//! with every item on the layer it was on (#469).

use pcbcraft_doc::Layer;

type T = Result<(), String>;

fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/four_layer_saved_before_stackup_names.pcbcraft")
}

#[test]
fn an_old_four_layer_file_opens_with_every_item_on_its_layer() -> T {
    let p = pcbcraft_io::load(&fixture())?;
    let layers: Vec<u8> = p.board.tracks.iter().map(|t| t.layer.index()).collect();
    assert_eq!(layers, vec![0, 1, 2, 3]);
    assert_eq!(p.board.tracks[3].layer, p.bottom_layer());
    assert_eq!(p.board.tracks[1].layer, Layer::Back);
    Ok(())
}

#[test]
fn resaving_an_old_file_names_the_bottom_and_keeps_the_layers() -> T {
    let p = pcbcraft_io::load(&fixture())?;
    let dir = std::env::temp_dir().join(format!("pcbcraft-stackup-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (one, two) = (dir.join("one.pcbcraft"), dir.join("two.pcbcraft"));
    pcbcraft_io::save(&p, &one)?;
    let text = std::fs::read_to_string(&one).map_err(|e| e.to_string())?;
    assert!(!text.contains("In3.Cu"), "the bottom is named B.Cu");
    assert!(text.contains("In1.Cu"), "index 1 is the first inner layer");
    let again = pcbcraft_io::load(&one)?;
    assert_eq!(again.board.tracks, p.board.tracks);
    pcbcraft_io::save(&again, &two)?;
    let same = std::fs::read(&one).map_err(|e| e.to_string())?
        == std::fs::read(&two).map_err(|e| e.to_string())?;
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    assert!(same, "a second save is byte-identical");
    Ok(())
}
