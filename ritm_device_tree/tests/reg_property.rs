use ritm_device_tree::fdt::Fdt;
use ritm_device_tree::model::DeviceTree;

#[test]
fn test_reg_property() {
    let dtb = include_bytes!("../dtb/test_reg_property.dtb");
    let fdt = Fdt::new(dtb.as_slice()).unwrap();
    let dt = DeviceTree::from_fdt(&fdt).unwrap();

    let root = dt.root();
    let child = root.children().next().unwrap();

    let reg = child.reg(root).unwrap();
    let mut reg_iter = reg.iter();

    let block1 = reg_iter.next().unwrap();
    assert_eq!(block1.address, 0x1000_0000_2000);
    assert_eq!(block1.size, Some(0x100));

    let block2 = reg_iter.next().unwrap();
    assert_eq!(block2.address, 0x3000_0000_4000);
    assert_eq!(block2.size, Some(0x200));

    assert!(reg_iter.next().is_none());
}
