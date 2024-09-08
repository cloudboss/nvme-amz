# nvme-amz

A Rust library to probe NVMe devices in Amazon EC2.

It provides functionality similar to that of the `ebsnvme-id` command but adds information about instance store devices, not only EBS.

To use this library, you must define one of the features `ioctl-nix` or `ioctl-rustix`. Either [nix](https://crates.io/crates/nix) or [rustix](https://crates.io/crates/rustix) will be pulled in as dependencies, respectively. The chosen library is used to make ioctl system calls.

The library implements `TryFrom<File>` for `Nvme`, so this is how to interact with it. For example:

```
use nvme_amz::Nvme;

fn main() {
    let file = File::open("/dev/nvme0").expect("unable to open device");
    let nvme: Nvme = file.try_into().expect("unable to probe device");
    println!("{:?}", nvme);
}
```

Output for an EBS volume:

```
Nvme { device: Device { device_name: Some("sda"), virtual_name: None }, model: AmazonElasticBlockStore, vendor_id: VendorId(7439) }
```

Output for an instance store volume:

```
Nvme { device: Device { device_name: Some("sdb"), virtual_name: Some("ephemeral0") }, model: AmazonNvmeInstanceStore, vendor_id: VendorId(7439) }
```
