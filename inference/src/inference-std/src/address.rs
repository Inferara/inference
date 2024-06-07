pub struct Address {
    value: usize,
    is_defined: bool,
    data: usize,
}

impl Address {
    pub fn new(value: usize) -> Self {
        Self {
            value,
            is_defined: true,
            data: 0,
        }
    }

    pub fn is_defined(&self) -> bool {
        self.is_defined
    }

    pub fn value(&self) -> usize {
        self.value
    }

    pub fn data(&self) -> usize {
        self.data
    }

    pub fn set_data(&mut self, data: usize) {
        self.data = data;
        self.is_defined = true;
    }
}
