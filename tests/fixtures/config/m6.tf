resource "aws_instance" "web" {
 tags = {
  Name = "needle-web"
 }
 ingress = [
  { port = 80 },
  { name = "needle" }
 ]
 script = <<-EOT
  echo needle ${not_a_block}
  EOT
}
