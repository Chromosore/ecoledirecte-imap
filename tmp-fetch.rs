
                // Ok c'est bon maintenant

                let body_needed = match macro_or_item_names {
                    Macro(m) => m == Macro::Full,
                    MessageDataItemNames(items) => items.contains(MessageDataItemName::Body),
                };

                let (min, max) =
                    sequence_set
                        .0
                        .as_ref()
                        .iter()
                        .fold((u32::MAX, 0), |minmax, sequence| match sequence {
                            Single(nr) => (minmax.0.min(nr), minmax.1.max(nr)),
                            Range(min, max) => (minmax.0.min(min), minmax.1.max(max)),
                        });

				let messages = mailbox::translate_to_mailbox(mailbox, api::get_folder_messages(mailbox::paginate(min, max), ));
                if body_needed {
                    let messages =
                    // On doit fetch chaque message individuellement
                    for message in sequence_set.iter() {}
                } else {
                    // On peut grouper la requête
                    // Solution naïve parce que flemme
                }
