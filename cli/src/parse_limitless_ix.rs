use clap::{Arg, ArgMatches, Command};
use crate::utils::get_shared_args;
use base58::FromBase58;
use limitless::instructions::LimitlessInstruction;

pub(crate) async fn parse_limitless_ix(args: &ArgMatches) {
    let data = FromBase58::from_base58(args.value_of("data").unwrap()).unwrap();
    println!("{:#?}", LimitlessInstruction::unpack(data.as_slice()).unwrap());
}


pub(crate) fn cmd() -> Command<'static> {
    Command::new("parse-limitless-ix").
        args(&get_shared_args()).
        arg(
            Arg::new("data").
                long("data").
                short('d').
                help("base58 encoded sata").
                takes_value(true).
                required(true)
        )
}