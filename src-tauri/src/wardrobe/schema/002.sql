CREATE INDEX garments_by_added ON garments(added_at DESC, id);
CREATE INDEX garments_by_category ON garments(category_id, added_at DESC, id);
CREATE INDEX garments_by_name ON garments(name COLLATE NOCASE, id);
