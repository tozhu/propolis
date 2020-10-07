extern crate bhyve_api;
extern crate pico_args;
#[macro_use]
extern crate bitflags;
extern crate byteorder;

mod devices;
mod dispatch;
mod exits;
mod intr_pins;
mod machine;
mod pci;
#[macro_use]
mod pio;
mod types;
mod util;
mod vcpu;
mod vm;

use std::fs::File;
use std::sync::Arc;

use bhyve_api::vm_reg_name;
use dispatch::*;
use exits::*;
use machine::{Machine, MachineCtx};
use pio::PioDev;
use vcpu::VcpuHdl;

use devices::uart::{LpcUart, COM1_IRQ, COM1_PORT};
use pci::PciBDF;

const PAGE_OFFSET: u64 = 0xfff;

struct Opts {
    rom: String,
    vmname: String,
}

fn parse_args() -> Opts {
    let mut args = pico_args::Arguments::from_env();
    let rom: String = args.value_from_str("-r").unwrap();
    let vmname: String = args.free().unwrap().pop().unwrap();
    Opts { rom, vmname }
}

struct PciLpcImpl;
impl pci::Device for PciLpcImpl {}

fn run_loop(dctx: DispCtx, mut vcpu: VcpuHdl) {
    let mctx = &dctx.mctx;
    let mut next_entry = VmEntry::Run;
    loop {
        let exit = vcpu.run(&next_entry).unwrap();
        //println!("rip:{:x} exit: {:?}", exit.rip, exit.kind);
        match exit.kind {
            VmExitKind::Bogus => {
                //println!("rip:{:x} exit: {:?}", exit.rip, exit.kind);
                next_entry = VmEntry::Run
            }
            VmExitKind::Inout(io) => match io {
                InoutReq::Out(io, val) => {
                    mctx.with_pio(|b| b.handle_out(io.port, io.bytes, val));
                    next_entry = VmEntry::InoutComplete(InoutRes::Out(io));
                }
                InoutReq::In(io) => {
                    let val = mctx.with_pio(|b| b.handle_in(io.port, io.bytes));
                    next_entry = VmEntry::InoutComplete(InoutRes::In(io, val));
                }
            },
            _ => panic!("unrecognized exit: {:?}", exit.kind),
        }
    }
}

fn main() {
    let opts = parse_args();

    let hdl = vm::create_vm(&opts.vmname).unwrap();
    println!("vm {} created", &opts.vmname);

    let vm = Machine::new(hdl, 1);

    let lowmem: usize = 512 * 1024 * 1024;
    vm.setup_lowmem(lowmem).unwrap();

    // Setup bootrom
    {
        let mut fp = File::open(&opts.rom).unwrap();
        let len = fp.metadata().unwrap().len();
        if len & PAGE_OFFSET != 0 {
            panic!("bad rom length {}", len);
        }
        vm.setup_bootrom(len as usize).unwrap();
        vm.populate_bootrom(&mut fp, len as usize).unwrap();
    }

    vm.initalize_rtc(lowmem).unwrap();

    vm.wire_pci_root();

    let mctx = MachineCtx::new(&vm);

    let com1 = LpcUart::new(COM1_IRQ);
    mctx.with_pio(|pio| pio.register(COM1_PORT, 8, pio_dyn!(com1.clone())));

    let lpc_dev = pci::Builder::new(pci::Ident {
        vendor_id: 0x8086,
        device_id: 0x7000,
        class: 0x06,
        subclass: 0x01,
        sub_vendor_id: 0,
        sub_device_id: 0,
    })
    // BAR, just for kicks
    //.add_bar_io(pci::BarN::BAR0, 0x100)
    .add_lintr()
    .finish(PciLpcImpl {});
    mctx.with_pci(|pci| pci.attach(PciBDF::new(0, 31, 0), lpc_dev));

    let mut vcpu0 = vm.vcpu(0);

    vcpu0.reboot_state().unwrap();
    vcpu0.activate().unwrap();
    vcpu0.set_reg(vm_reg_name::VM_REG_GUEST_RIP, 0xfff0).unwrap();

    let mut dispatch = Dispatcher::new(mctx);

    dispatch.spawn_events().unwrap();
    dispatch.spawn_vcpu(vcpu0, run_loop).unwrap();

    dispatch.join();

    drop(vm);
}
