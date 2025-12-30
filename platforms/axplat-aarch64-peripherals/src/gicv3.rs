//! ARM Generic Interrupt Controller (GIC).
//! GICv3
#![allow(non_upper_case_globals, dead_code)]

use aarch64_cpu::registers::Readable;
use arm_gic::{
    IntId,
    gicv3::{GicV3, InterruptGroup, SgiTarget, SgiTargetGroup},
    irq_enable,
};
use axplat::irq::{HandlerTable, IpiTarget, IrqHandler};
use axplat::mem::VirtAddr;
use kspin::SpinNoIrq;
use lazyinit::LazyInit;

/// The maximum number of IRQs.
const MAX_IRQ_COUNT: usize = 1024;
/// The ID of the first Software Generated Interrupt.
const SGI_START: u32 = 0;
/// The ID of the first Private Peripheral Interrupt.
const PPI_START: u32 = 16;
/// The ID of the first Shared Peripheral Interrupt.
const SPI_START: u32 = 32;
/// The first special interrupt ID.
const SPECIAL_START: u32 = 1020;
/// One more than the last special interrupt ID.
const SPECIAL_END: u32 = 1024;
/// The first extended Private Peripheral Interrupt.
const EPPI_START: u32 = 1056;
/// One more than the last extended Private Peripheral Interrupt.
const EPPI_END: u32 = 1120;
/// The first extended Shared Peripheral Interrupt.
const ESPI_START: u32 = 4096;
/// One more than the last extended Shared Peripheral Interrupt.
const ESPI_END: u32 = 5120;
/// The first Locality-specific Peripheral Interrupt.
const LPI_START: u32 = 8192;

static GICv3: LazyInit<SpinNoIrq<GicV3>> = LazyInit::new();

static IRQ_HANDLER_TABLE: HandlerTable<MAX_IRQ_COUNT> = HandlerTable::new();

/// Enables or disables the given IRQ.
pub fn set_enable(irq_num: usize, enabled: bool) {
    let irq_num = irq_num as u32;
    let int_id = if irq_num < PPI_START {
        IntId::sgi(irq_num - SGI_START)
    } else if irq_num < SPI_START {
        IntId::ppi(irq_num - PPI_START)
    } else if irq_num < SPECIAL_START {
        IntId::spi(irq_num - SPI_START)
    } else if (irq_num >= EPPI_START) && (irq_num < EPPI_END) {
        IntId::eppi(irq_num - EPPI_START)
    } else if (irq_num >= ESPI_START) && (irq_num < ESPI_END) {
        IntId::espi(irq_num - ESPI_START)
    } else if irq_num >= LPI_START {
        IntId::lpi(irq_num - LPI_START)
    } else {
        error!("Invalid IRQ number: {}", irq_num);
        return;
    };

    // Sets the priority mask for the current CPU core.
    GicV3::set_priority_mask(0xff);

    let mpidr_el1 = aarch64_cpu::registers::MPIDR_EL1.get() as usize;
    let cpu_id = mpidr_el1 & 0xff; // fix me
    debug!(
        "GICv3 set enable: irq={} {} to CPU {}",
        Into::<u32>::into(int_id),
        enabled,
        cpu_id
    );
    GICv3
        .lock()
        .set_interrupt_priority(int_id, Some(cpu_id), 0x80);

    GICv3.lock().enable_interrupt(int_id, Some(cpu_id), true);
}

/// Send SGI to target CPU
/// Sends an inter-processor interrupt (IPI) to the specified target CPU or all CPUs.
pub fn send_ipi(irq_num: usize, target: IpiTarget) {
    if irq_num >= PPI_START as usize {
        error!("unknown SGI number: {}", irq_num);
        return;
    }

    let mpidr_el1 = aarch64_cpu::registers::MPIDR_EL1.get();
    let _aff0 = (mpidr_el1 & 0xff) as u8;
    let aff1 = ((mpidr_el1 >> 8) & 0xff) as u8;
    let aff2 = ((mpidr_el1 >> 16) & 0xff) as u8;
    let aff3 = ((mpidr_el1 >> 32) & 0xff) as u8;

    let sgi_intid = IntId::sgi(irq_num as u32);
    let sgi_target = match target {
        IpiTarget::Current { cpu_id } => {
            debug!("Send SGI {} to current CPU {}", irq_num, cpu_id);
            SgiTarget::List {
                affinity3: aff3,
                affinity2: aff2,
                affinity1: aff1,
                target_list: 0b1 << cpu_id,
            }
        }
        IpiTarget::Other { cpu_id } => {
            debug!("Send SGI {} to CPU {}", irq_num, cpu_id);
            SgiTarget::List {
                affinity3: aff3,
                affinity2: aff2,
                affinity1: aff1,
                target_list: 0b1 << cpu_id,
            }
        }
        IpiTarget::AllExceptCurrent {
            cpu_id: _,
            cpu_num: _,
        } => {
            debug!("Send SGI {} to all CPU", irq_num);
            SgiTarget::All
        }
    };

    // Sends a software-generated interrupt (SGI) to the given cores.
    GicV3::send_sgi(sgi_intid, sgi_target, SgiTargetGroup::CurrentGroup1);
}

/// Registers an IRQ handler for the given IRQ.
///
/// It also enables the IRQ if the registration succeeds. It returns `false`
/// if the registration failed.
pub fn register_handler(irq_num: usize, handler: IrqHandler) -> bool {
    debug!("register handler IRQ {}", irq_num);
    if IRQ_HANDLER_TABLE.register_handler(irq_num, handler) {
        set_enable(irq_num, true);
        return true;
    }
    warn!("register handler for IRQ {} failed", irq_num);
    false
}

/// Unregisters the IRQ handler for the given IRQ.
///
/// It also disables the IRQ if the unregistration succeeds. It returns the
/// existing handler if it is registered, `None` otherwise.
pub fn unregister_handler(irq_num: usize) -> Option<IrqHandler> {
    trace!("unregister handler IRQ {}", irq_num);
    set_enable(irq_num, false);
    IRQ_HANDLER_TABLE.unregister_handler(irq_num)
}

/// Handles the IRQ.
///
/// It is called by the common interrupt handler. It should look up in the
/// IRQ handler table and calls the corresponding handler. If necessary, it
/// also acknowledges the interrupt controller after handling.
pub fn handle_irq(_unused: usize) -> Option<usize> {
    // Read IAR
    if let Some(int_id) = GicV3::get_and_acknowledge_interrupt(InterruptGroup::Group1) {
        let num = Into::<u32>::into(int_id) as usize;
        trace!(
            "CPU {:x} got irq: {}",
            aarch64_cpu::registers::MPIDR_EL1.get(),
            num
        );
        if !IRQ_HANDLER_TABLE.handle(num) {
            warn!("IRQ_HANDLER_TABLE unhandled IRQ {}", num);
        }

        // Write EOIR
        GicV3::end_interrupt(int_id, InterruptGroup::Group1);

        Some(num)
    } else {
        warn!("Didn't handle an irq");
        None
    }
}

/// Initializes GICv3 (for the primary CPU only).
pub fn init_gicv3(
    gicd_base: VirtAddr,
    gicr_base: VirtAddr,
    _cpu: usize,
    cpu_count: usize,
    gic_v4: bool,
) {
    info!("Initialise the GICv3");
    GICv3.init_once(SpinNoIrq::new(unsafe {
        GicV3::new(
            gicd_base.as_mut_ptr() as _,
            gicr_base.as_mut_ptr() as _,
            cpu_count,
            gic_v4,
        )
    }));
}

/// Initializes GICC (for all CPUs).
/// `cpu` should be the linear index of the CPU core as used by the GIC redistributor.
/// It must be called after [`init_GICv3`].
pub fn init_gicc(cpu: usize) {
    // Initialises the GIC and marks the given CPU core as awake.
    // and enable group 1 for the current security state.
    GICv3.lock().setup(cpu); // init_cpu
    GICv3.lock().gicd_barrier();
    GICv3.lock().gicr_barrier(cpu);
}

/// Default implementation of [`axplat::irq::IrqIf`] using the GIC.
#[macro_export]
macro_rules! irq_if_impl {
    ($name:ident) => {
        struct $name;

        #[impl_plat_interface]
        impl axplat::irq::IrqIf for $name {
            /// Enables or disables the given IRQ.
            fn set_enable(irq: usize, enabled: bool) {
                $crate::gicv3::set_enable(irq, enabled);
            }

            /// Registers an IRQ handler for the given IRQ.
            ///
            /// It also enables the IRQ if the registration succeeds. It returns `false`
            /// if the registration failed.
            fn register(irq: usize, handler: axplat::irq::IrqHandler) -> bool {
                $crate::gicv3::register_handler(irq, handler)
            }

            /// Unregisters the IRQ handler for the given IRQ.
            ///
            /// It also disables the IRQ if the unregistration succeeds. It returns the
            /// existing handler if it is registered, `None` otherwise.
            fn unregister(irq: usize) -> Option<axplat::irq::IrqHandler> {
                $crate::gicv3::unregister_handler(irq)
            }

            /// Handles the IRQ.
            ///
            /// It is called by the common interrupt handler. It should look up in the
            /// IRQ handler table and calls the corresponding handler. If necessary, it
            /// also acknowledges the interrupt controller after handling.
            fn handle(irq: usize) -> Option<usize> {
                $crate::gicv3::handle_irq(irq)
            }

            /// Sends an inter-processor interrupt (IPI) to the specified target CPU or all CPUs.
            fn send_ipi(irq_num: usize, target: axplat::irq::IpiTarget) {
                $crate::gicv3::send_ipi(irq_num, target);
            }
        }
    };
}
