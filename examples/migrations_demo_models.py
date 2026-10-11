"""
Models for examples/migrations_demo.py.

Set CELLO_DEMO_STAGE=2 to get the second version of the models (a new field, a
new model and a changed constraint), so the demo can show an incremental
migration.
"""

import os

from cello.orm import (
    AutoField, BooleanField, CharField, ForeignKey, IntegerField, Model, TextField,
)

STAGE = int(os.environ.get("CELLO_DEMO_STAGE", "1"))


class Author(Model):
    id = AutoField()
    name = CharField(max_length=100)

    class Meta:
        table_name = "ex_authors"


class Book(Model):
    id = AutoField()
    title = CharField(max_length=200, unique=True)
    author = ForeignKey(Author, on_delete="CASCADE")
    if STAGE >= 2:
        pages = IntegerField(default=0)          # new column
        blurb = TextField(null=True)             # new nullable column

    class Meta:
        table_name = "ex_books"


if STAGE >= 2:
    class Review(Model):                          # new table
        id = AutoField()
        book = ForeignKey(Book, on_delete="CASCADE")
        stars = IntegerField()
        public = BooleanField(default=True)

        class Meta:
            table_name = "ex_reviews"
