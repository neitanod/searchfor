package server

type Server struct {
	Addr string // needle address
}

func (s *Server) Start() error {
	return listen(s.Addr, "needle")
}

func helper() string {
	return "needle"
}
