use anyhow::Result;
use evdev::{
    uinput::VirtualDeviceBuilder, AttributeSet, BusType, EventType, InputEvent, InputId, Key,
    RelativeAxisType, Synchronization,
};

pub struct VirtualDevice {
    device: evdev::uinput::VirtualDevice,
}

impl VirtualDevice {
    pub fn new(enable_scroll: bool, enable_pointer: bool) -> Result<Self> {
        let mut keys = AttributeSet::<Key>::new();
        keys.insert(Key::BTN_LEFT);

        let mut rel_axes = AttributeSet::<RelativeAxisType>::new();

        if enable_scroll {
            rel_axes.insert(RelativeAxisType::REL_WHEEL);
            rel_axes.insert(RelativeAxisType::REL_HWHEEL);
            rel_axes.insert(RelativeAxisType::REL_WHEEL_HI_RES);
            rel_axes.insert(RelativeAxisType::REL_HWHEEL_HI_RES);
        }

        if enable_pointer {
            rel_axes.insert(RelativeAxisType::REL_X);
            rel_axes.insert(RelativeAxisType::REL_Y);
        }

        let device = VirtualDeviceBuilder::new()?
            .name("rinertia Virtual Device")
            .input_id(InputId::new(BusType::BUS_VIRTUAL, 0, 0, 1))
            .with_keys(&keys)?
            .with_relative_axes(&rel_axes)?
            .build()?;

        Ok(Self { device })
    }

    pub fn emit_scroll(&mut self, axis: crate::ScrollAxis, hires_value: i32) -> Result<()> {
        let (hires_axis, lores_axis) = match axis {
            crate::ScrollAxis::Vertical => (
                RelativeAxisType::REL_WHEEL_HI_RES,
                RelativeAxisType::REL_WHEEL,
            ),
            crate::ScrollAxis::Horizontal => (
                RelativeAxisType::REL_HWHEEL_HI_RES,
                RelativeAxisType::REL_HWHEEL,
            ),
        };

        let mut events = vec![InputEvent::new(
            EventType::RELATIVE,
            hires_axis.0,
            hires_value,
        )];

        let lores = hires_value / 120;
        if lores != 0 {
            events.push(InputEvent::new(EventType::RELATIVE, lores_axis.0, lores));
        }

        events.push(InputEvent::new(
            EventType::SYNCHRONIZATION,
            Synchronization::SYN_REPORT.0,
            0,
        ));

        self.device.emit(&events)?;
        Ok(())
    }

    pub fn emit_pointer(&mut self, dx: i32, dy: i32) -> Result<()> {
        let events = [
            InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_X.0, dx),
            InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_Y.0, dy),
            InputEvent::new(EventType::SYNCHRONIZATION, Synchronization::SYN_REPORT.0, 0),
        ];
        self.device.emit(&events)?;
        Ok(())
    }
}
