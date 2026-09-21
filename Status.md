# What is this?

This document serves to reming me where I left off when I last worked on this project

# 2026-09-14
TODO:
- fix core-operators tests
    - probably remove Debug trait from KVT
- wire the internals section into the website?
    - want to have a mermaid diagram with StartBuild protocol visualized
- finish the OperatorBuilder refactoring:
    - refactor with the derive(Builder) pattern?
    - hide Operator internals behind the Builder methods
    - especially look at union and split operators -- new API has to work for them

# 2026-01-04

I do not understand where I left off, I am currently working on re-creating the code for the distributed
and ICA stuff, but currently only mocking out the pub/pub(crate) APIs

Below is the general architecture I apparently came up with last time with three operators

input_recv
 - in:
    - local messages
    - remote messages
- out:
    versioned messages with sender
- tasks:
    - keep client set up to date
    - align barrier
 
state-handler:
    - in:
        versioned messages with sender
    - out:
        versioned message with sender
    - tasks:
        - run ICA algorithm
        - buffer collected messages
        
output_send:
    - in:
        versioned message with sender
    - out:
        - Wiremessage (remote)
        - normal message (local)
    - tasks:
        - route messages
        - use correct router in ICA process
