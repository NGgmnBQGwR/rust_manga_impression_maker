use crate::types::{MangaEntry, MangaGroup, MangaImage};

static TEMPLATE: &str = include_str!("template.html");

static SECTION_ELEMENT: &str = r#"
<section data-transition-speed="fast">
    <h3>{{title}}</h3>
    <p>{{score}}/10</p>
    <p>{{comment}}</p>
    {{image_stack}}
</section>
"#;

static IMAGE_ELEMENT: &str = r#"
    <div class="r-stack r-stretch">
        {{image_elements}}
    </div>
"#;

pub struct MangaGroupExporter<'a> {
    group: MangaGroup,
    entries: Vec<(MangaEntry, Vec<MangaImage>)>,
    handlebars: handlebars::Handlebars<'a>,
    cwd: std::path::PathBuf,
    export_path: std::path::PathBuf,
}

impl<'a> MangaGroupExporter<'a> {
    pub fn new(group: MangaGroup, mut entries: Vec<(MangaEntry, Vec<MangaImage>)>) -> Self {
        let mut handlebars = handlebars::Handlebars::new();
        handlebars.set_strict_mode(true);
        handlebars.register_escape_fn(handlebars::no_escape);
        handlebars
            .register_template_string("main_template", TEMPLATE)
            .unwrap();
        handlebars
            .register_template_string("section_template", SECTION_ELEMENT)
            .unwrap();
        handlebars
            .register_template_string("image_template", IMAGE_ELEMENT)
            .unwrap();

        entries.sort_by(|a, b| a.0.score.cmp(&b.0.score));

        Self {
            group,
            entries,
            handlebars,
            cwd: std::env::current_dir().unwrap(),
            export_path: std::env::current_dir().unwrap(),
        }
    }

    fn _copy_image(&self, image: &MangaImage) -> String {
        let full_path_from = self.cwd.join(&image.path);
        let relative_folder_to = std::path::PathBuf::new()
            .join("media")
            .join(format!("review_{}", self.group.id));
        let full_folder_to = self.export_path.parent().unwrap().join(&relative_folder_to);
        if !full_folder_to.exists() {
            std::fs::create_dir_all(&full_folder_to).unwrap();
        }

        let filename = full_path_from.file_name().unwrap().to_string_lossy();
        let full_path_to = full_folder_to.join(&*filename);
        std::fs::copy(&full_path_from, full_path_to).unwrap();
        relative_folder_to
            .join(&*filename)
            .to_string_lossy()
            .into_owned()
    }

    fn _create_image_element(&self, images: &[MangaImage]) -> String {
        match images.len() {
            0 => String::new(),
            1 => format!(
                r#"<div class="r-stretch"><img src="{}"></div>"#,
                self._copy_image(&images[0])
            ),
            _ => {
                let mut elements = Vec::with_capacity(images.len());
                for (index, elem) in images.iter().enumerate() {
                    match index {
                        0 => elements.push(format!(r#"<img class="fragment fade-out" data-fragment-index="0" src="{}">"#, self._copy_image(elem))),
                        1 => elements.push(format!(r#"<img class="fragment fade-in-then-out" data-fragment-index="0" src="{}">"#, self._copy_image(elem))),
                        _ => elements.push(format!(r#"<img class="fragment fade-in-then-out" src="{}">"#, self._copy_image(elem))),
                    }
                }
                let mut data = std::collections::HashMap::new();
                data.insert("image_elements", elements.join("\n"));

                self.handlebars.render("image_template", &data).unwrap()
            }
        }
    }

    fn _create_manga_element(&self, manga: &MangaEntry, images: &[MangaImage]) -> String {
        let image_element = self._create_image_element(images);
        let mut data = std::collections::HashMap::new();
        data.insert("title", manga.name.clone());
        data.insert("score", manga.score.to_string());
        data.insert("comment", manga.comment.clone());
        data.insert("image_stack", image_element);
        self.handlebars.render("section_template", &data).unwrap()
    }

    pub fn export_group(&mut self) {
        let date = chrono::Local::now().date_naive();

        let export_filepath = rfd::FileDialog::new()
            .set_title("Select export destination")
            .set_directory(std::env::current_dir().unwrap())
            .add_filter("HTML file", &["html"])
            .set_file_name(&format!("{}_{}.html", date, self.group.id))
            .save_file();

        if export_filepath.is_none() {
            return;
        }

        self.export_path = export_filepath.unwrap();

        let mut elements = Vec::with_capacity(self.entries.len());
        for (manga, images) in &self.entries {
            elements.push(self._create_manga_element(manga, images));
        }

        let mut data = std::collections::HashMap::new();
        data.insert(
            "title",
            format!("Manga review #{} ({})", self.group.id, self.group.added_on),
        );
        data.insert("sections", elements.join("\n"));
        let result = self.handlebars.render("main_template", &data).unwrap();

        std::fs::write(&self.export_path, result).unwrap();
    }
}
