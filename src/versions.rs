use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct Version{
	#[allow(dead_code)]
	name: String,
	#[allow(dead_code)]
	tag_name: String,
	#[allow(dead_code)]
	created_at: String,
	link: String,
	installer_link: String,
	#[allow(dead_code)]
	git_commit_url: String,
	#[allow(dead_code)]
	archived: bool,
}
//create getters for all fields
impl Version{
	#[allow(dead_code)]
	pub fn new(name: String, tag_name: String, created_at: String, link: String, installer_link: String, git_commit_url: String, archived: bool) -> Self{
		Self{
			name,
			tag_name,
			created_at,
			link,
			installer_link,
			git_commit_url,
			archived,
		}
	}
	#[allow(dead_code)]
	pub fn get_name(&self) -> &String{
		&self.name
	}
	#[allow(dead_code)]
	pub fn get_tag_name(&self) -> &String{
		&self.tag_name
	}
	#[allow(dead_code)]
	pub fn get_created_at(&self) -> &String{
		&self.created_at
	}
	pub fn get_link(&self) -> &String{
		&self.link
	}
	pub fn get_installer_link(&self) -> &String{
		&self.installer_link
	}
	#[allow(dead_code)]
	pub fn get_git_commit_url(&self) -> &String{
		&self.git_commit_url
	}
	#[allow(dead_code)]
	pub fn get_archived(&self) -> &bool{
		&self.archived
	}
}
