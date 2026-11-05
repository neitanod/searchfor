@section('content')
<div class="cart">
@foreach($items as $item)
<li>{{ $item->needle }}</li>
@endforeach
</div>
@endsection
