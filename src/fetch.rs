use serde_json::Value;
use imap_codec::imap_types::fetch::MacroOrMessageDataItemNames, Macro, MessageDataItemName};

pub fn query(seq: u32, message: Value, query: MacroOrMessageDataItemNames) {
    let data_items = match query {
        Macro(macro) => macro.expand(),
        MessageDataItemName(data_items) => data_items,
    };

    Response::Data(Data::Fetch(seq, data_items.map(|item| {
        match item {
            Body => todo!(),
            BodyExt { section, partial, peek } => todo!(),
            BodyStucture => todo!(),
            Envelope => todo!(),
            Flags => todo!(),
            InternalDate => todo!(),
            Rfc822 => todo!(),
            Rfc822Header => todo!(),
            Rfc822Size => todo!(),
            Rfc822Text => todo!(),
            Uid => Uid(),
        }
    }))
}
