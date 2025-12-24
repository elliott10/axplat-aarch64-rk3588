use axplat::init::InitIf;

#[allow(unused_imports)]
use crate::config::devices::{
    GICC_PADDR, GICD_PADDR, GICR_PADDR, RTC_PADDR, TIMER_IRQ, UART_IRQ, UART_PADDR,
};
use crate::config::plat::{CPU_NUM, PSCI_METHOD};
use axplat::mem::{pa, phys_to_virt};

struct InitIfImpl;

#[impl_plat_interface]
impl InitIf for InitIfImpl {
    /// Initializes the platform at the early stage for the primary core.
    ///
    /// This function should be called immediately after the kernel has booted,
    /// and performed earliest platform configuration and initialization (e.g.,
    /// early console, clocking).
    fn init_early(_cpu_id: usize, _dtb: usize) {
        axcpu::init::init_trap();
        axplat_aarch64_peripherals::pl011::init_early(phys_to_virt(pa!(UART_PADDR)));
        axplat_aarch64_peripherals::psci::init(PSCI_METHOD);
        axplat_aarch64_peripherals::generic_timer::init_early();
        #[cfg(feature = "rtc")]
        axplat_aarch64_peripherals::pl031::init_early(phys_to_virt(pa!(RTC_PADDR)));
    }

    /// Initializes the platform at the early stage for secondary cores.
    #[cfg(feature = "smp")]
    fn init_early_secondary(_cpu_id: usize) {
        axcpu::init::init_trap();
    }

    /// Initializes the platform at the later stage for the primary core.
    ///
    /// This function should be called after the kernel has done part of its
    /// initialization (e.g, logging, memory management), and finalized the rest of
    /// platform configuration and initialization.
    fn init_later(cpu_id: usize, _dtb: usize) {
        #[cfg(feature = "irq")]
        {
            #[cfg(not(feature = "gicv3"))]
            {
                axplat_aarch64_peripherals::gic::init_gic(
                    phys_to_virt(pa!(GICD_PADDR)),
                    phys_to_virt(pa!(GICC_PADDR)),
                );
                axplat_aarch64_peripherals::gic::init_gicc();
            }
            #[cfg(feature = "gicv3")]
            {
                axplat_aarch64_peripherals::gicv3::init_gicv3(
                    phys_to_virt(pa!(GICD_PADDR)),
                    phys_to_virt(pa!(GICR_PADDR)),
                    cpu_id,
                    CPU_NUM,
                    false,
                );
                axplat_aarch64_peripherals::gicv3::init_gicc(cpu_id);
            }

            // cpu0 handle timer irq bug. Fix me
            //axplat_aarch64_peripherals::generic_timer::enable_irqs(TIMER_IRQ);

            // enable UART IRQs
            axplat::irq::register(UART_IRQ, axplat_aarch64_peripherals::pl011::irq_handler);
        }
    }

    /// Initializes the platform at the later stage for secondary cores.
    #[cfg(feature = "smp")]
    fn init_later_secondary(cpu_id: usize) {
        #[cfg(feature = "irq")]
        {
            #[cfg(not(feature = "gicv3"))]
            axplat_aarch64_peripherals::gic::init_gicc();
            #[cfg(feature = "gicv3")]
            axplat_aarch64_peripherals::gicv3::init_gicc(cpu_id);

            axplat_aarch64_peripherals::generic_timer::enable_irqs(TIMER_IRQ);
        }

        // Test SGI, need to enable sgi
        // axplat_aarch64_peripherals::gicv3::send_ipi(1, axplat::irq::IpiTarget::Other{cpu_id: 0});
    }
}
