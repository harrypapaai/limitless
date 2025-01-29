extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, DeriveInput, ItemStruct, Ident, ItemEnum};

#[proc_macro_derive(Packable)]
pub fn packable_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let expanded = quote! {
        impl Packable for #name {
            fn pack(&self, account_info: &solana_program::account_info::AccountInfo)
                -> Result<(), solana_program::program_error::ProgramError>
            {
                let dst = &mut &mut account_info.data.borrow_mut()[..];
                self.serialize(dst).map_err(|_| solana_program::program_error::ProgramError::InvalidAccountData)
            }

            fn serialize_vec(&self) -> Result<Vec<u8>, solana_program::program_error::ProgramError> {
                let mut result = Vec::with_capacity(1024);
                self.serialize(&mut result).map_err(|_| solana_program::program_error::ProgramError::InvalidAccountData)?;
                Ok(result)
            }

            fn unpack(account_info: &solana_program::account_info::AccountInfo)
                -> Result<Self, solana_program::program_error::ProgramError>
            {
                if account_info.data_is_empty() {
                    return Err(solana_program::program_error::ProgramError::UninitializedAccount);
                }
                let data = &mut &account_info.data.borrow()[..];
                Self::deserialize(data)
                    .map_err(|_| solana_program::program_error::ProgramError::InvalidAccountData)
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(ToAccountMetaList)]
pub fn to_account_meta_list_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let fields = match input.data {
        syn::Data::Struct(s) => s.fields,
        _ => panic!("Only structs are supported"),
    };

    let fields_mapped = fields.iter().map(|f| {
        let field_name = f.clone().ident.unwrap();
        quote! {
            self.#field_name.into()
        }
    });

    let expanded = quote! {
        impl ToAccountMetaList for #name {
            fn to_account_meta_list(&self) -> Vec<solana_program::instruction::AccountMeta>
            {
                vec![
                    #(#fields_mapped),*
                ]
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn account_infos_struct(attr: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    let name = parse_macro_input!(attr as Ident);
    let fields = input.fields.iter().map(|f| {
        let name = f.clone().ident;
        quote!{
            pub #name: &'slice solana_program::account_info::AccountInfo<'info>
        }
    });
    let vec_values = input.fields.iter().map(|f| {
        let name = f.clone().ident;
        quote!{
            self.#name.clone()
        }
    });

    let expanded = quote! {
        pub struct #name<'slice, 'info: 'slice> {
            #(#fields),*
        }

        impl<'slice, 'info: 'slice> #name<'slice, 'info> {
            pub fn to_vec(&self) -> Vec<solana_program::account_info::AccountInfo<'info>> {
                vec![
                    #(#vec_values),*
                ]
            }
        }

        #input
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn instruction_args(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);
    let enum_name = input.ident.clone();
    let arg_structs = input.variants.iter().map(|v| {
        let name = v.ident.clone();
        let struct_name = format_ident!("{}Args", name.clone());

        let field_declr = match &v.fields {
            syn::Fields::Named(f) => f.named.iter().map(|f| {
                let field = f.clone();
                let field_name = field.ident.unwrap();
                let field_type = field.ty;
                quote!{
                    pub #field_name: #field_type
                }
            }).collect::<Vec<proc_macro2::TokenStream>>(),
            syn::Fields::Unit => Vec::new(),
            _ => panic!("Unamed struct variants are supported"),
        };

        let field_sets = match &v.fields {
            syn::Fields::Named(f) => f.named.iter().map(|f| {
                let field_name = f.clone().ident.unwrap();
                quote!{
                    #field_name: self.#field_name
                }
            }).collect::<Vec<proc_macro2::TokenStream>>(),
            syn::Fields::Unit => Vec::new(),
            _ => panic!("Unamed struct variants are supported"),
        };

        quote! {
            #[derive(Clone, Debug)]
            pub struct #struct_name {
                #(#field_declr),*
            }
            impl Into<#enum_name> for #struct_name {
                fn into(self) -> #enum_name {
                    #enum_name::#name{
                        #(#field_sets),*
                    }
                }
            }
        }
    });

    let expanded = quote! {
        #(#arg_structs)*

        #input
    };

    TokenStream::from(expanded)
}


#[proc_macro_attribute]
pub fn blackwing_event(
    _args: TokenStream,
    input: TokenStream,
) -> TokenStream {
    let event_strct = parse_macro_input!(input as syn::ItemStruct);

    let event_name = &event_strct.ident;

    let discriminator: proc_macro2::TokenStream = {
        let discriminator_preimage = format!("event:{event_name}").into_bytes();
        let discriminator = anchor_syn::hash::hash(&discriminator_preimage);
        format!("{:?}", &discriminator.0[..8]).parse().unwrap()
    };

    let ret = quote! {
        #[derive(BorshSerialize, BorshDeserialize, Debug)]
        #event_strct

        impl #event_name {
            const DISCRIMINATOR: [u8; 8] = #discriminator;
        }

        impl ToEventData for #event_name {
            fn data(&self) -> Vec<u8> {
                let mut data = vec![];
                data.extend_from_slice(&#discriminator);
                self.serialize(&mut data).unwrap();
                data
            }
        }
    };

    TokenStream::from(ret)
}
