// rustfmt-normalize_doc_attributes: true

#[doc = "..."]
static S: () = {
    #[doc = "..."]
    struct S;
};

#[doc= "..."]
static S: () = {
    #[doc= "..."]
    struct S;
};

#[doc="..."]
static S: () = {
    #[doc="..."]
    struct S;
};
