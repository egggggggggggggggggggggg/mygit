CREATE TABLE comment_files (
   comment_id UUID NOT NULL
      REFERENCES comments(id) ON DELETE CASCADE,

   file_id UUID NOT NULL
      REFERENCES files(id) ON DELETE CASCADE,
   -- Denotes the order in which images appear in a commennt. eg. 0 means first image, 1 means second, etc. 
   position INT NOT NULL DEFAULT 0,

   PRIMARY KEY (comment_id, file_id)
);
